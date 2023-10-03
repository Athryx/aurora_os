use core::slice;

use crate::alloc::{HeapRef, PaRef};
use crate::arch::x64::{IntDisable, asm_thread_init};
use crate::cap::address_space::AddressSpace;
use crate::cap::capability_space::CapabilitySpace;
use crate::int::IPI_PROCESS_EXIT;
use crate::int::apic::{Ipi, IpiDest};
use crate::cap::{CapObject, CapType};
use crate::prelude::*;
use crate::container::{Arc, Weak};
use crate::sync::IMutex;
use super::{Thread, ThreadState, PostSwitchAction, KernelStack, switch_current_thread_to, thread_map};

/// Passed to create_thread to specify which state thread should start in
#[derive(Debug, Clone, Copy)]
pub enum ThreadStartMode {
    Ready,
    Suspended,
}

/// A thread group can contain either another thread goup or a thread
#[derive(Debug)]
pub enum ThreadGroupChild {
    // FIXME: need to remove these from the list otherwise they cause a memory leak (thread group is dropped but memory is not reclaimed)
    // due to the usage patterns of thread group, this could happen quite often
    ThreadGroup(Weak<ThreadGroup>),
    Thread(Arc<Thread>),
}

/// Capability that allows spawning processess, and manages destroying process groups
// FIXME: figure out how drop will work
#[derive(Debug)]
pub struct ThreadGroup {
    thread_list: IMutex<Vec<ThreadGroupChild>>,
    heap_allocator: HeapRef,
    page_allocator: PaRef,
}

impl ThreadGroup {
    pub fn new(page_allocator: PaRef, heap_allocator: HeapRef) -> Self {
        ThreadGroup {
            thread_list: IMutex::new(Vec::new(heap_allocator.clone())),
            heap_allocator,
            page_allocator,
        }
    }

    pub fn add_thread(&self, thread: Arc<Thread>) -> KResult<()> {
        self.thread_list.lock().push(ThreadGroupChild::Thread(thread))
    }

    /// Searches the thread list for the given thread and removes it
    pub fn remove_thread(&self, thread: &Arc<Thread>) {
        let mut thread_list = self.thread_list.lock();

        for (i, thread_group_child) in thread_list.iter().enumerate() {
            if let ThreadGroupChild::Thread(child_thread) = thread_group_child {
                if Arc::ptr_eq(thread, child_thread) {
                    thread_list.remove(i);
                    return;
                }
            }
        }
    }

    pub fn create_thread(
        this: &Arc<Self>,
        address_space: Arc<AddressSpace>,
        capability_space: Arc<CapabilitySpace>,
        name: String,
        start_mode: ThreadStartMode,
        rip: usize,
        rsp: usize,
    ) -> KResult<Arc<Thread>> {
        let kernel_stack = KernelStack::new(this.page_allocator.clone())?;

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

        let kernel_rsp = kernel_stack.stack_top() - 8 * push_index;
        let thread = Arc::new(
            Thread::new(
                name,
                kernel_stack,
                kernel_rsp.as_usize(),
                Arc::downgrade(this),
                address_space,
                capability_space,
                this.heap_allocator.clone(),
            ),
            this.heap_allocator.clone(),
        )?;

        let mut thread_list = this.thread_list.lock();
        thread_list.push(ThreadGroupChild::Thread(thread.clone()))?;

        // insert thread handle into scheduler after all other setup is done
        match start_mode {
            ThreadStartMode::Ready => {
                thread.set_state(ThreadState::Ready);

                if let Err(error) = thread_map().insert_ready_thread(Arc::downgrade(&thread)) {
                    thread_list.pop();
                    return Err(error);
                }
            },
            ThreadStartMode::Suspended => {
                thread.set_state(ThreadState::Suspended);
            },
        }

        Ok(thread)
    }

    pub fn create_child_thread_group(&self, page_allocator: PaRef, heap_allocator: HeapRef) -> KResult<Arc<Self>> {
        let thread_group = Arc::new(
            Self::new(page_allocator, heap_allocator.clone()),
            heap_allocator,
        )?;

        self.thread_list.lock().push(ThreadGroupChild::ThreadGroup(Arc::downgrade(&thread_group)))?;

        Ok(thread_group)
    }

    /// Kills all threads in this thread group, including the current thread
    pub fn exit(this: Arc<Self>) {
        let kill_self = this.exit_inner();

        cpu_local_data().local_apic().send_ipi(Ipi::To(IpiDest::AllExcludeThis, IPI_PROCESS_EXIT));

        drop(this);

        if kill_self {
            switch_current_thread_to(
                ThreadState::Dead,
                // creating a new int disable is fine, we don't care to restore interrupts because this thread will die
                IntDisable::new(),
                PostSwitchAction::None,
                false,
            ).unwrap();
        }
    }

    /// Kills all threads that this thread group or its child thread groups contain
    /// 
    /// # Returns
    /// 
    /// true if the current thread is in this group, which means the caller should kill itself
    fn exit_inner(&self) -> bool {
        let mut thread_list = self.thread_list.lock();

        let mut kill_self = false;

        while let Some(child) = thread_list.pop() {
            match child {
                ThreadGroupChild::Thread(thread) => {
                    thread.set_dead();
                    if thread.is_current_thread() {
                        kill_self = true;
                    }
                },
                // FIXME: security: this could cause infinite recursion and stack overflow
                // don't use recursion here
                ThreadGroupChild::ThreadGroup(thread_group) => {
                    let Some(thread_group) = thread_group.upgrade() else {
                        continue;
                    };

                    if thread_group.exit_inner() {
                        kill_self = true;
                    }
                }
            }
        }

        kill_self
    }
}

impl Drop for ThreadGroup {
    // This doesn't kill the current thread, so it will run a bit before scheduler decides to switch to another thread
    // TODO: figure out how to have drop communicate to switch to new thread
    fn drop(&mut self) {
        let _kill_self = self.exit_inner();

        cpu_local_data().local_apic().send_ipi(Ipi::To(IpiDest::AllExcludeThis, IPI_PROCESS_EXIT));
    }
}

impl CapObject for ThreadGroup {
    const TYPE: CapType = CapType::ThreadGroup;
}