use core::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use core::cmp::min;
use core::slice;

use spin::Once;
use sys::MemoryResizeFlags;

use crate::arch::x64::{asm_thread_init, IntDisable};
use crate::cap::memory::{Memory, MemoryInner};
use crate::container::{Arc, Weak, HashMap};
use crate::int::IPI_PROCESS_EXIT;
use crate::int::apic::{Ipi, IpiDest};
use crate::sched::{Tid, Thread, ThreadState, PostSwitchAction, thread_map, switch_current_thread_to};
use crate::alloc::{PaRef, HeapRef, root_alloc_page_ref, root_alloc_ref};
use crate::cap::{CapFlags, CapObject, StrongCapability, WeakCapability, CapabilitySpace, CapType, CapId};
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

/// Represents where in the address space a capability was mapped
#[derive(Debug)]
struct AddrSpaceMapping {
    addr: VirtAddr,
    size_pages: usize,
    flags: PageMappingFlags,
}

/// Stores data related to the virtual address space of the process
#[derive(Debug)]
struct AddrSpaceData {
    addr_space: VirtAddrSpace,
    /// A map between Memory CapIds to the address at which they are mapped
    mapped_memory_capabilities: HashMap<CapId, AddrSpaceMapping>,
}

impl AddrSpaceData {
    fn update_memory_mapping_inner(
        &mut self,
        memory_cap_id: CapId,
        memory_inner: &mut MemoryInner,
        max_size_pages: Option<usize>
    ) -> KResult<usize> {
        let Some(mapping) = self.mapped_memory_capabilities.get(&memory_cap_id) else {
            // memory was not yet mapped
            return Err(SysErr::InvlOp);
        };

        let old_size = mapping.size_pages;
        let new_size = max_size_pages.unwrap_or(mapping.size_pages);
        if new_size == 0 {
            return Err(SysErr::InvlArgs);
        }

        if new_size > old_size {
            let new_base_addr = mapping.addr + old_size;

            let mapping_iter = memory_inner.iter_mapped_regions(
                new_base_addr,
                Size::zero(),
                Size::from_pages(new_size - old_size),
            );

            // must map new regions first before resizing old mapping
            let flags = mapping.flags | PageMappingFlags::USER;
            self.addr_space.map_many(
                mapping_iter.clone().without_unaligned_start(),
                flags,
            )?;

            let result = self.addr_space.resize_mapping(mapping_iter.get_entire_first_maping_range());

            if let Err(error) = result {
                for (virt_range, _) in mapping_iter {
                    // panic safety: this memory was just mapped so this is guarenteed to not fail
                    self.addr_space.unmap_memory(virt_range).unwrap();
                }

                Err(error)
            } else {
                Ok(new_size)
            }
        } else if new_size < old_size {
            let unmap_base_addr = mapping.addr + new_size;

            let mapping_iter = memory_inner.iter_mapped_regions(
                unmap_base_addr,
                Size::zero(),
                Size::from_pages(old_size - new_size),
            );

            // first resize the overlapping part
            self.addr_space.resize_mapping(mapping_iter.get_first_mapping_exluded_range())?;

            // now unmap everything else
            for (virt_range, _) in mapping_iter.without_unaligned_start() {
                // panic safety: this memory should be mapped
                self.addr_space.unmap_memory(virt_range).unwrap();
            }

            Ok(new_size)
        } else {
            Ok(old_size)
        }
    }
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
    heap_allocator: HeapRef,

    pub is_alive: AtomicBool,
    pub num_threads_running: AtomicUsize,

    strong_reference: IMutex<Option<Arc<Self>>>,
    self_weak: Once<Weak<Self>>,

    /// Counter used to assign thread ids
    next_tid: AtomicUsize,
    threads: IMutex<HashMap<Tid, Arc<Thread>>>,

    addr_space_data: IMutex<AddrSpaceData>,
    cr3_addr: PhysAddr,
    cap_map: CapabilitySpace,
}

impl Process {
    pub fn new(page_allocator: PaRef, allocer: HeapRef, name: String) -> KResult<WeakCapability<Self>> {
        let addr_space = VirtAddrSpace::new(page_allocator.clone(), allocer.clone())?;

        let strong_cap = StrongCapability::new_flags(
            Arc::new(Process {
                name,
                page_allocator,
                heap_allocator: allocer.clone(),
                is_alive: AtomicBool::new(true),
                num_threads_running: AtomicUsize::new(0),
                strong_reference: IMutex::new(None),
                self_weak: Once::new(),
                next_tid: AtomicUsize::new(0),
                threads: IMutex::new(HashMap::new(allocer.clone())),
                cr3_addr: addr_space.cr3_addr(),
                addr_space_data: IMutex::new(AddrSpaceData {
                    addr_space,
                    mapped_memory_capabilities: HashMap::new(allocer.clone()),
                }),
                cap_map: CapabilitySpace::new(allocer.clone()),
            }, allocer)?,
            CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
        );

        *strong_cap.object().strong_reference.lock() = Some(strong_cap.inner().clone());
        strong_cap.object().self_weak.call_once(|| Arc::downgrade(strong_cap.inner()));

        Ok(strong_cap.downgrade())
    }
    
    pub fn page_allocator(&self) -> PaRef {
        self.page_allocator.clone()
    }

    pub fn heap_allocator(&self) -> HeapRef {
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

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns a reference to the capability map of this process
    pub fn cap_map(&self) -> &CapabilitySpace {
        &self.cap_map
    }

    /// Releases the strong capbility for the process, which will lead to the process being dropped when all other strong referenes are dropped
    pub fn release_strong_capability(&self) {
        *self.strong_reference.lock() = None;
    }

    /// Gets a unique valid Tid
    fn next_tid(&self) -> Tid {
        Tid::from(self.next_tid.fetch_add(1, Ordering::Relaxed))
    }

    /// Crates a new idle thread structure for the currently running thread
    /// 
    /// `stack` should be a virt range referencing the whole stack of the current thread
    /// 
    /// # Locking
    /// 
    /// acquires the `threads` lock
    pub fn create_idle_thread(&self, name: String, stack: AVirtRange) -> KResult<Arc<Thread>> {
        let thread = Thread::new(
            self.next_tid(),
            name,
            self.self_weak(),
            KernelStack::Existing(stack),
            // rsp will be set on thread switch, so it can be 0 for now
            0,
        )?;

        self.threads.lock().insert(thread.tid, thread.clone())?;
        // idle thread should be currently running
        self.num_threads_running.fetch_add(1, Ordering::AcqRel);

        Ok(thread)
    }

    /// Creates a new thread
    /// 
    /// The thread will return to userspace code at rip upon starting
    /// 
    /// Rsp will be initialized, as well as 4 general purpose registers
    /// 
    /// # Locking
    /// 
    /// acquires the `threads` lock
    pub fn create_thread(
        &self,
        name: String,
        start_mode: ThreadStartMode,
        rip: usize,
        rsp: usize,
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
            stack_slice[stack_slice.len() - 1 - push_index] = val;
            push_index += 1;
        };

        // setup stack the first thing the new thread does is
        // load the specified registers and jump to userspace code
        push(rsp);
        push(rip);
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
        let thread = Thread::new(
            tid,
            name,
            self.self_weak(),
            kernel_stack,
            kernel_rsp.as_usize(),
        )?;

        let mut threads = self.threads.lock();
        threads.insert(thread.tid, thread.clone())?;

        // insert thread handle into scheduler after all other setup is done
        match start_mode {
            ThreadStartMode::Ready => {
                thread.set_state(ThreadState::Ready);

                if let Err(error) = thread_map().insert_ready_thread(Arc::downgrade(&thread)) {
                    threads.remove(&thread.tid);
                    return Err(error);
                }
            },
            ThreadStartMode::Suspended => {
                thread.set_state(ThreadState::Suspended);
            },
        }

        Ok(tid)
    }

    /// Resumes a thread with the given thread id if it was suspended
    /// 
    /// # Locking
    /// 
    /// acquires the `threads` lock
    pub fn resume_suspended_thread(&self, thread_id: Tid) -> KResult<()> {
        let threads = self.threads.lock();
        let thread = threads.get(&thread_id).ok_or(SysErr::InvlId)?;

        if thread.transition_state(ThreadState::Suspended, ThreadState::Ready) {
            // FIXME: don't panic on oom
            thread_map().insert_ready_thread(Arc::downgrade(thread))
                .expect("could not resume suepended thread");

            Ok(())
        } else {
            Err(SysErr::InvlOp)
        }
    }

    /// Destroys a thread with the given thread id if it was suspended
    /// 
    /// # Locking
    /// 
    /// acquires the `threads` lock
    pub fn destroy_suspended_thread(&self, thread_id: Tid) -> KResult<()> {
        let mut threads = self.threads.lock();
        let thread = threads.get(&thread_id).ok_or(SysErr::InvlId)?;

        if thread.transition_state(ThreadState::Suspended, ThreadState::Dead) {
            threads.remove(&thread_id);
            Ok(())
        } else {
            Err(SysErr::InvlOp)
        }
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
    /// so no locks or refcounted objects should be held when calling this,
    /// unless it has already been checked that `this` is not the current process
    /// 
    /// # Locking
    /// 
    /// acquires `local_apic` lock
    pub fn exit(this: Arc<Process>) {
        if !this.is_alive.swap(false, Ordering::AcqRel) {
            // another thread is already terminating this process
            return;
        }

        this.release_strong_capability();

        cpu_local_data().local_apic().send_ipi(Ipi::To(IpiDest::AllExcludeThis, IPI_PROCESS_EXIT));

        if this.is_current_process() {
            drop(this);

            switch_current_thread_to(
                ThreadState::Dead,
                // creating a new int disable is fine, we don't care to restore interrupts because this thread will die
                IntDisable::new(),
                PostSwitchAction::None,
                false,
            ).unwrap();
        }
    }

    /// Maps the memory specified by the given capability at the given virtual address
    /// 
    /// returns the size in pages of the memory that was mapped
    /// 
    /// `memory` must reference a strong capability
    /// 
    /// if `max_size_pages` is `Some(_)`, the mapped memory will take up no more than `max_size_pages` pages in the virtual address space
    /// 
    /// `flags` specifies the read, write, and execute permissions, but the memory is always mapped as user
    /// Returns invalid args if not bits in falgs are set
    /// 
    /// # Locking
    /// 
    /// acquires `addr_space_data` lock
    pub fn map_memory(
        &self,
        memory: StrongCapability<Memory>,
        addr: VirtAddr,
        max_size_pages: Option<usize>,
        flags: PageMappingFlags
    ) -> KResult<usize> {
        assert!(memory.references_strong());

        let mut addr_space_data = self.addr_space_data.lock();

        if addr_space_data.mapped_memory_capabilities.get(&memory.id).is_some() {
            // memory is already mapped
            return Err(SysErr::InvlOp);
        }

        let mut memory_inner = memory.object().inner_write();

        let size_pages = min(max_size_pages.unwrap_or(memory_inner.size_pages()), memory_inner.size_pages());
        if size_pages == 0 {
            return Err(SysErr::InvlArgs);
        }

        addr_space_data.mapped_memory_capabilities.insert(memory.id, AddrSpaceMapping {
            addr,
            size_pages,
            flags,
        })?;

        let map_result = addr_space_data.addr_space.map_many(
            memory_inner.iter_mapped_regions(addr, Size::zero(), Size::from_pages(size_pages)),
            flags | PageMappingFlags::USER,
        );

        if let Err(error) = map_result {
            // if mapping failed, remove entry from mapped_memory_capabilities
            addr_space_data.mapped_memory_capabilities.remove(&memory.id);

            Err(error)
        } else {
            memory_inner.map_ref_count += 1;

            Ok(size_pages)
        }
    }

    /// Unmaps the memory specified by the given capability if it was already mapped with [`map_memory`]
    /// 
    /// # Locking
    /// 
    /// acquires `addr_space_data` lock
    /// acquires the `inner` lock on the memory capability
    pub fn unmap_memory(&self, memory: StrongCapability<Memory>) -> KResult<()> {
        assert!(memory.references_strong());

        let mut addr_space_data = self.addr_space_data.lock();
        let mut memory_inner = memory.object().inner_write();

        let Some(mapping) = addr_space_data.mapped_memory_capabilities.remove(&memory.id) else {
            // memory was not yet mapped
            return Err(SysErr::InvlOp);
        };

        for (virt_range, _) in memory_inner.iter_mapped_regions(
            mapping.addr,
            Size::zero(),
            Size::from_pages(mapping.size_pages),
        ) {
            // this should not fail because we ensure that memory was already mapped
            addr_space_data.addr_space.unmap_memory(virt_range)
                .expect("failed to unmap memory that should have been mapped");
        }

        memory_inner.map_ref_count -= 1;

        Ok(())
    }

    /// Updates the mapping for the given memory capability
    /// 
    /// # Returns
    /// 
    /// Returns the size of the new mapping in pages
    /// 
    /// # Locking
    /// 
    /// acquires `addr_apce_data` lock
    /// acquires the `inner` lock on the moemry capability
    pub fn update_memory_mapping(
        &self,
        memory: StrongCapability<Memory>,
        max_size_pages: Option<usize>,
    ) -> KResult<usize> {
        assert!(memory.references_strong());

        let mut addr_space_data = self.addr_space_data.lock();
        let mut memory_inner = memory.object().inner_write();

        addr_space_data.update_memory_mapping_inner(memory.id, &mut memory_inner, max_size_pages)
    }

    /// Resizes the specified memory capability specified by `memory` to be the size of `new_size_pages`
    /// 
    /// If `resize_in_place` is true, the memory can be resized even if it is currently mapped
    /// 
    /// # Returns
    /// 
    /// returns the new size of the memory in pages
    /// 
    /// # Locking
    /// 
    /// acquires `addr_space_data` lock
    /// acquires the `inner` lock on the memory capability
    pub fn resize_memory(&self, memory: StrongCapability<Memory>, new_page_size: usize, flags: MemoryResizeFlags) -> KResult<usize> {
        let mut addr_space_data = self.addr_space_data.lock();
        let mut memory_inner = memory.object().inner_write();

        let old_page_size = memory_inner.size_pages();

        if old_page_size == new_page_size {
            return Ok(old_page_size);
        }

        if memory_inner.map_ref_count == 0 {
            // Safety: map ref count is checked to be 0, os this capability is not mapped in memory
            unsafe {
                memory_inner.resize_out_of_place(new_page_size)?;
            }

            Ok(memory_inner.size_pages())
        } else if flags.contains(MemoryResizeFlags::IN_PLACE) && memory_inner.map_ref_count == 1 {
            let Some(mapping) = addr_space_data.mapped_memory_capabilities.get(&memory.id) else {
                return Err(SysErr::InvlOp);
            };

            if new_page_size > old_page_size {
                unsafe {
                    memory_inner.resize_in_place(new_page_size)?;
                }

                let memory_size = memory_inner.size_pages();
                if flags.contains(MemoryResizeFlags::GROW_MAPPING) {
                    addr_space_data.update_memory_mapping_inner(
                        memory.id,
                        &mut memory_inner,
                        Some(memory_size)
                    )?;
                }

                Ok(memory_size)
            } else if new_page_size < old_page_size {
                // shrink memory
                if mapping.size_pages > new_page_size {
                    addr_space_data.update_memory_mapping_inner(
                        memory.id,
                        &mut memory_inner,
                        Some(new_page_size)
                    )?;
                }
                
                // panic safety: shrinking the allocated memory should never fail
                unsafe {
                    memory_inner.resize_in_place(new_page_size).unwrap();
                }

                Ok(memory_inner.size_pages())
            } else {
                Ok(memory_inner.size_pages())
            }
        } else {
            Err(SysErr::InvlOp)
        }
    }
}

impl CapObject for Process {
    const TYPE: CapType = CapType::Process;
}

static KERNEL_PROCESS: Once<Arc<Process>> = Once::new();

/// Initializes the kernel process
pub fn init_kernel_process() {
    const FAIL_MESSAGE: &str = "could not initialize kernel process";

    KERNEL_PROCESS.call_once(|| {
        Process::new(
            root_alloc_page_ref(),
            root_alloc_ref(),
            String::from_str(root_alloc_ref(), "kernel")
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