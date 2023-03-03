use core::sync::atomic::{AtomicUsize, AtomicBool};

use crate::container::Arc;
use crate::mem::MemOwner;
use super::kernel_stack::KernelStack;
use crate::container::{ListNode, ListNodeData, Weak};
use crate::process::Process;
use crate::prelude::*;

crate::make_id_type!(Tid);

#[derive(Debug, Clone, Copy)]
pub enum ThreadState {
    Running,
    Ready,
    Dead {
        // if true, the scheduler will check that this is the 
		// last thread switching away from a dead process,
		// and will destoy the process as well
        try_destroy_process: bool
    },
    // if for_event for either suspend is false,
	// the scheduler will not ehck the capid field on the thread,
	// and will assume it is not waiting for an event to improve performance
    Suspend {
        for_event: bool,
    },
    SuspendTimeout {
        for_event: bool,
        until_nanosecond: u64,
    },
}

#[derive(Debug)]
pub struct Thread {
    tid: Tid,
    // FIXME: maybe handle setting this to null when thread handle dropped (see if it is an issue)
    handle: *const ThreadHandle,
    pub process: Weak<Process>,
    // this has to be atomic usize because it is written to in assembly
    pub rsp: AtomicUsize,
    // if this is non zero, the scheduler will exchange this field with 0 when switching away from a suspend state,
    // if waiting_capid is already 0, the scheduler knows some other code is already switching the task to ready
    pub waiting_capid: AtomicUsize,
    kernel_stack: KernelStack,
}

#[derive(Debug)]
pub struct ThreadHandle {
    pub state: ThreadState,
    pub thread: Arc<Thread>,

    list_node_data: ListNodeData<ThreadHandle>,
}

impl ThreadHandle {
    /// Creates a new thread handle
    /// 
    /// Uses the allocator from `Arc<Thread>`
    pub fn new(thread: Arc<Thread>) -> KResult<MemOwner<ThreadHandle>> {
        let mut allocator = Arc::alloc_ref(&thread);

        MemOwner::new(
            ThreadHandle {
                state: ThreadState::Ready,
                thread,
                list_node_data: ListNodeData::default(),
            },
            allocator.allocator(),
        )
    }

    /// Deallocates the thread handle
    /// 
    /// # Safety
    /// 
    /// No other references to the thread handle can exist
    pub unsafe fn dealloc(thread_handle: MemOwner<ThreadHandle>) {
        let mut allocator = Arc::alloc_ref(&thread_handle.thread);

        unsafe {
            thread_handle.drop_in_place(allocator.allocator());
        }
    }
}

impl ListNode for ThreadHandle {
    fn list_node_data(&self) -> &ListNodeData<Self> {
        &self.list_node_data
    }
}
