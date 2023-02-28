use core::sync::atomic::{AtomicUsize, AtomicBool};

use crate::alloc::OrigAllocator;
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
    handle: *const ThreadHandle,
    pub process: Weak<Process>,
    // this has to be atomic usize because it is written to in assembly
    pub rsp: AtomicUsize,
    // if this is true, the scheduler will exchange this field with false when switching away from a suspend state,
    // if waiting_on_capid is already false, the scheduler knows some other code is already switching the task to ready
    pub waiting_on_capid: AtomicBool,
    kernel_stack: KernelStack,
}

#[derive(Debug)]
pub struct ThreadHandle {
    pub state: ThreadState,
    pub thread: Arc<Thread>,

    list_node_data: ListNodeData<ThreadHandle>,
}

impl ThreadHandle {
    pub fn new(thread: Arc<Thread>, allocator: &dyn OrigAllocator) -> KResult<MemOwner<ThreadHandle>> {
        MemOwner::new(
            ThreadHandle {
                state: ThreadState::Ready,
                thread,
                list_node_data: ListNodeData::default(),
            },
            allocator,
        )
    }
}

impl ListNode for ThreadHandle {
    fn list_node_data(&self) -> &ListNodeData<Self> {
        &self.list_node_data
    }
}
