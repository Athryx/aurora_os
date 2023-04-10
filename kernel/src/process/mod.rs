use core::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use core::slice;

use spin::Once;

use crate::arch::x64::{asm_thread_init, IntDisable};
use crate::container::{Arc, Weak, HashMap};
use crate::int::IPI_PROCESS_EXIT;
use crate::int::apic::{Ipi, IpiDest};
use crate::mem::{MemOwner, addr};
use crate::sched::{Tid, Thread, ThreadHandle, ThreadState, PostSwitchAction, THREAD_MAP, switch_current_thread_to};
use crate::alloc::{PaRef, OrigRef, root_alloc_page_ref, root_alloc_ref};
use crate::cap::{CapFlags, CapObject, StrongCapability, WeakCapability, CapabilityMap, CapType, CapId, Capability};
use crate::prelude::*;
use crate::sched::kernel_stack::KernelStack;
use crate::sync::IMutex;

mod spawner;
pub use spawner::Spawner;
mod vmem_manager;
pub use vmem_manager::{VirtAddrSpace, PageMappingFlags};

/// Passed to create_thread to specify which state thread should start in
#[derive(Debug, Clone, Copy)]
pub enum ThreadStartMode {
    Ready,
    Suspended,
}

/// Stores data related to the virtual address space of the process
#[derive(Debug)]
struct AddrSpaceData {
    addr_space: VirtAddrSpace,
    /// A map between Memory CapIds to the address at which they are mapped
    mapped_memory_capabilities: HashMap<CapId, VirtAddr>,
}

impl Drop for AddrSpaceData {
    fn drop(&mut self) {
        unsafe {
            self.addr_space.dealloc_addr_space();
        }
    }
}

/// A capability that represents a protection context, has a set of capabilities and a virtual address space
#[derive(Debug)]
pub struct Process {
    name: String,

    page_allocator: PaRef,
    heap_allocator: OrigRef,

    pub is_alive: AtomicBool,
    pub num_threads_running: AtomicUsize,

    strong_reference: IMutex<Option<Arc<Self>>>,
    self_weak: Once<Weak<Self>>,

    /// Counter used to assign thread ids
    next_tid: AtomicUsize,
    threads: IMutex<Vec<Arc<Thread>>>,

    addr_space_data: IMutex<AddrSpaceData>,
    cr3_addr: PhysAddr,
    cap_map: CapabilityMap,
}

impl Process {
    pub fn new(page_allocator: PaRef, allocer: OrigRef, name: String) -> KResult<WeakCapability<Self>> {
        let addr_space = VirtAddrSpace::new(page_allocator.clone(), allocer.downgrade())?;

        let strong_cap = StrongCapability::new(
            Process {
                name,
                page_allocator: page_allocator,
                heap_allocator: allocer.clone(),
                is_alive: AtomicBool::new(true),
                num_threads_running: AtomicUsize::new(0),
                strong_reference: IMutex::new(None),
                self_weak: Once::new(),
                next_tid: AtomicUsize::new(0),
                threads: IMutex::new(Vec::new(allocer.clone().downgrade())),
                cr3_addr: addr_space.cr3_addr(),
                addr_space_data: IMutex::new(AddrSpaceData {
                    addr_space,
                    mapped_memory_capabilities: HashMap::new(allocer.downgrade()),
                }),
                cap_map: CapabilityMap::new(allocer.downgrade()),
            },
            CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
            allocer,
        )?;

        *strong_cap.object().strong_reference.lock() = Some(strong_cap.inner().clone());
        strong_cap.object().self_weak.call_once(|| Arc::downgrade(strong_cap.inner()));

        Ok(strong_cap.downgrade())
    }
    
    pub fn page_allocator(&self) -> PaRef {
        self.page_allocator.clone()
    }

    pub fn heap_allocator(&self) -> OrigRef {
        self.heap_allocator.clone()
    }

    pub fn self_weak(&self) -> Weak<Self> {
        self.self_weak.get().unwrap().clone()
    }

    /// Returns the value that should be loaded in the cr3 register
    /// 
    /// This is the pointer to the top lavel paging table for the process
    pub fn get_cr3(&self) -> usize {
        self.cr3_addr.as_usize()
    }

    /// Returns a reference to the capability map of this process
    pub fn cap_map(&self) -> &CapabilityMap {
        &self.cap_map
    }

    /// Releases the strong capbility for the process, which will lead to the process being destroyed
    /// 
    /// # Safety
    /// 
    /// Don't do this with any of the process' threads running
    pub unsafe fn release_strong_capability(&self) {
        *self.strong_reference.lock() = None;
    }

    /// Gets a unique valid Tid
    fn next_tid(&self) -> Tid {
        Tid::from(self.next_tid.fetch_add(1, Ordering::Relaxed))
    }

    /// Inserts the thread into the thread list
    /// 
    /// The thread list is sorted by Tid
    fn insert_thread(&self, thread: Arc<Thread>) -> KResult<()> {
        let mut thread_list = self.threads.lock();

        let insert_index = thread_list
            .binary_search_by_key(&thread.tid, |thread| thread.tid)
            .expect_err("duplicate tids detected");

        thread_list.insert(insert_index, thread)
    }

    /// Crates a new idle thread structure for the currently running thread
    /// 
    /// `stack` should be a virt range referencing the whole stack of the current thread
    pub fn create_idle_thread(&self, name: String, stack: AVirtRange) -> KResult<(Arc<Thread>, MemOwner<ThreadHandle>)> {
        let (thread, thread_handle) = Thread::new(
            self.next_tid(),
            name,
            self.self_weak(),
            KernelStack::Existing(stack),
            // rsp will be set on thread switch, so it can be 0 for now
            0,
        )?;

        self.insert_thread(thread.clone())?;

        Ok((thread, thread_handle))
    }

    /// Creates a new thread
    /// 
    /// The thread will return to userspace code at rip upon starting
    /// 
    /// Rsp will be initialized, as well as 4 general purpose registers
    pub fn create_thread(
        &self,
        name: String,
        start_mode: ThreadStartMode,
        rip: usize,
        rsp: usize,
        regs: (usize, usize, usize, usize)
    ) -> KResult<Tid> {
        let kernel_stack = KernelStack::new(self.page_allocator())?;

        // safety: kernel_stack points to valid memory
        let stack_slice = unsafe { 
            slice::from_raw_parts_mut(
                kernel_stack.stack_base().as_mut_ptr(),
                kernel_stack.as_virt_range().size() / size_of::<usize>(),
            )
        };

        let mut push_index = 0;
        let mut push = |val: usize| {
            stack_slice[stack_slice.len() - 1 -push_index] = val;
            push_index += 1;
        };

        // setup stack the first thing the new thread does is
        // load the specified registers and jump to userspace code
        push(rsp);
        push(rip);
        push(regs.3);
        push(regs.2);
        push(regs.1);
        push(regs.0);
        push(asm_thread_init as usize);
        push(0);
        push(0);
        push(0);
        push(0);
        push(0);
        push(0);
        push(0x202);

        let tid = self.next_tid();
        let kernel_rsp = kernel_stack.stack_top() - 8 * push_index;
        let (thread, thread_handle) = Thread::new(
            tid,
            name,
            self.self_weak(),
            kernel_stack,
            kernel_rsp.as_usize(),
        )?;

        self.insert_thread(thread)?;

        // insert thread handle into scheduler after all other setup is done
        match start_mode {
            ThreadStartMode::Ready => THREAD_MAP.insert_ready_thread(thread_handle),
            ThreadStartMode::Suspended => THREAD_MAP.insert_suspended_thread(thread_handle),
        }

        Ok(tid)
    }

    pub fn is_current_process(&self) -> bool {
        let current_addr = cpu_local_data().current_process_addr.load(Ordering::Acquire);
        current_addr == self as *const _ as usize
    }

    /// Trigger the process to exit
    /// 
    /// This will stop all running threads from this process, and drop the process eventually
    /// 
    /// This may trigger the current thread to die if it is exiting its own process,
    /// so no locks or refcounted objects hould be held when calling this,
    /// unless it has already been checked that `this` is not the current process
    /// 
    /// # Locking
    /// 
    /// acquires `local_apic` lock
    pub fn exit(this: Arc<Process>) {
        if !this.is_alive.swap(false, Ordering::AcqRel) {
            // another thread is already terminating this process
            return
        }

        cpu_local_data().local_apic().send_ipi(Ipi::To(IpiDest::AllExcludeThis, IPI_PROCESS_EXIT));

        if this.is_current_process() {
            // wait for all other threads except this one to exit
            while this.num_threads_running.load(Ordering::Acquire) != 1 {}

            drop(this);

            switch_current_thread_to(
                ThreadState::Dead { try_destroy_process: true },
                // creating a new int disable is fine, we don't care to restore interrupts because this thread will die
                IntDisable::new(),
                PostSwitchAction::None
            ).unwrap();
        } else {
            // wait for all other threads to exit
            while this.num_threads_running.load(Ordering::Acquire) != 0 {}

            // safety: no other threads from this process are running
            unsafe {
                this.release_strong_capability();
            }
        }
    }

    /// Maps the memory specified by the given capid at the given virtual address
    /// 
    /// `memory_cap_id` must reference a capability already present in this process
    /// returns the size in pages of the memory that was mapped
    /// 
    /// `flags` specifies the read, write, and execute permissions, but the memory is always mapped as user
    /// Returns invalid args if not bits in falgs are set
    /// 
    /// # Locking
    /// 
    /// acquires `addr_space_data` lock
    pub fn map_memory(&self, memory_cap_id: CapId, addr: VirtAddr, flags: PageMappingFlags) -> KResult<usize> {
        let memory = self.cap_map.get_memory(memory_cap_id)?;

        let mut addr_space_data = self.addr_space_data.lock();

        if let Capability::Strong(memory) = memory {
            if addr_space_data.mapped_memory_capabilities.get(&memory_cap_id).is_some() {
                // memory is already mapped
                return Err(SysErr::InvlOp);
            }

            let mut memory_inner = memory.object().inner();

            let mem_virt_range = AVirtRange::try_new_aligned(
                addr,
                memory_inner.size(),
            ).ok_or(SysErr::InvlAlign)?;

            addr_space_data.mapped_memory_capabilities.insert(memory_cap_id, addr)?;

            let map_result = addr_space_data.addr_space.map_memory(
                &[(mem_virt_range, memory_inner.phys_addr())],
                flags | PageMappingFlags::USER,
            );

            if let Err(error) = map_result {
                // if mapping failed, remove entry from mapped_memory_capabilities
                addr_space_data.mapped_memory_capabilities.remove(&memory_cap_id);

                Err(error)
            } else {
                memory_inner.map_ref_count += 1;

                Ok(memory_inner.size_pages())
            }
        } else {
            Err(SysErr::InvlWeak)
        }
    }

    /// Unmaps the memory specified by the given capid if it was already mapped with [`map_memory`]
    /// 
    /// `memory_cap_id` must reference a capability already present in this process
    /// 
    /// # Locking
    /// 
    /// acquires `addr_space_data` lock
    pub fn unmap_memory(&self, memory_cap_id: CapId) -> KResult<()> {
        let memory = self.cap_map.get_memory(memory_cap_id)?;

        let mut addr_space_data = self.addr_space_data.lock();

        if let Capability::Strong(memory) = memory {
            if let Some(map_addr) = addr_space_data.mapped_memory_capabilities.get(&memory_cap_id) {
                let mut memory_inner = memory.object().inner();

                let mem_virt_range = AVirtRange::try_new_aligned(
                    *map_addr,
                    memory_inner.size(),
                ).ok_or(SysErr::InvlAlign)?;

                // this should not fail because we ensore that memory was already mapped
                addr_space_data.addr_space.unmap_memory(&[(mem_virt_range, memory_inner.phys_addr())])
                    .expect("failed to unmap memory that should have been mapped");

                addr_space_data.mapped_memory_capabilities.remove(&memory_cap_id);

                memory_inner.map_ref_count -= 1;

                Ok(())
            } else {
                // memory was not yet mapped
                Err(SysErr::InvlOp)
            }
        } else {
            Err(SysErr::InvlWeak)
        }
    }
}

impl CapObject for Process {
    const TYPE: CapType = CapType::Process;
}

static KERNEL_PROCESS: Once<Arc<Process>> = Once::new();

/// Initializes the kernel process
pub fn init_kernel_process() {
    const FAIL_MESSAGE: &'static str = "could not initialize kernel process";

    KERNEL_PROCESS.call_once(|| {
        Process::new(
            root_alloc_page_ref(),
            root_alloc_ref(),
            String::from_str(root_alloc_ref().downgrade(), "kernel")
                .expect(FAIL_MESSAGE)
        ).expect(FAIL_MESSAGE)
            .inner()
            .upgrade()
            .expect(FAIL_MESSAGE)
    });
}

/// Gets the kernel process, and panics if it has not yet been initialized
/// 
/// The kernel process just has all the idle threads for each cpu
pub fn get_kernel_process() -> Arc<Process> {
    KERNEL_PROCESS.get().unwrap().clone()
}