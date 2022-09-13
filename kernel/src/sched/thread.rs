use core::sync::atomic::AtomicPtr;
use core::fmt;

use crate::container::Weak;
use crate::process::Process;
use super::stack::Stack;

#[derive(Debug, Clone, Copy)]
struct Regsiters {
    rax: usize,
    rbx: usize,
    rcx: usize,
    rdx: usize,
    rdi: usize,
    rsi: usize,
    rbp: usize,
    rsp: usize,
    r8: usize,
    r9: usize,
    r10: usize,
    r11: usize,
    r12: usize,
    r13: usize,
    r14: usize,
    r15: usize,
    rflags: usize,
    rip: usize,
    cs: usize,
    ss: usize,
    // TODO: maybe save other segment registers?
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
    regs: Regsiters,
    stack: Stack,

    prev: AtomicPtr<Thread>,
    next: AtomicPtr<Thread>,
}

crate::impl_list_node!(Thread, prev, next);