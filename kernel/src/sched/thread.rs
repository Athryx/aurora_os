use crate::container::Arc;
use super::kernel_stack::KernelStack;
use crate::container::{ListNode, ListNodeData, Weak};
use crate::process::Process;

crate::make_id_type!(Tid);

#[derive(Debug, Clone, Copy)]
pub enum ThreadState {
    Running,
    Ready,
    Dead {
        try_destroy_process: bool
    },
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
    rsp: usize,
    kernel_stack: KernelStack,
}

#[derive(Debug)]
pub struct ThreadHandle {
    state: ThreadState,
    thread: Arc<Thread>,

    list_node_data: ListNodeData<ThreadHandle>,
}

impl ListNode for ThreadHandle {
    fn list_node_data(&self) -> &ListNodeData<Self> {
        &self.list_node_data
    }
}
