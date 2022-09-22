use crate::container::{Weak, ListNodeData, ListNode};
use crate::process::Process;
use super::stack::Stack;

#[derive(Debug, Clone, Copy)]
pub struct Registers {
    pub rax: usize,
    pub rbx: usize,
    pub rcx: usize,
    pub rdx: usize,
    pub rdi: usize,
    pub rsi: usize,
    pub rbp: usize,
    pub rsp: usize,
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

pub enum ThreadState {
    Running,
    Ready,
    // Thread is waiting to be killed
    Dead,
}

#[derive(Debug)]
pub struct Thread {
    tid: Tid,
    pub process: Weak<Process>,
    regs: Registers,
    stack: Stack,

    list_node_data: ListNodeData<Thread>,
}

impl ListNode for Thread {
    fn list_node_data(&self) -> &ListNodeData<Self> {
        &self.list_node_data
    }
}