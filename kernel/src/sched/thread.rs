use crate::container::Arc;
use super::kernel_stack::KernelStack;
use crate::container::{ListNode, ListNodeData, Weak};
use crate::process::Process;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Registers {
    pub rax: usize,
    pub rbx: usize,
    pub rcx: usize,
    pub rdx: usize,
    pub rbp: usize,
    pub rsp: usize,
    /// FIXME: these don't belong here
    /// This is the start fo the kernel stack, used by syscalls to load kernel stack
    pub kernel_rsp: usize,
    /// This is the saved rsp of a userspace thread when it makes a syscall
    pub call_save_rsp: usize,
    pub rdi: usize,
    pub rsi: usize,
    pub r8: usize,
    pub r9: usize,
    pub r10: usize,
    pub r11: usize,
    pub r12: usize,
    pub r13: usize,
    pub r14: usize,
    pub r15: usize,
    pub rflags: usize,
    pub rip: usize,
    pub cs: usize,
    pub ss: usize,
}

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
    regs: Registers,
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
