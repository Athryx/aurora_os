use core::sync::atomic::{AtomicUsize, Ordering};

use sys::{EventData, ThreadExit};

use crate::arch::x64::{wrmsr, FSBASE_MSR};
use crate::cap::CapObject;
use crate::cap::capability_space::CapabilitySpace;
use crate::cap::address_space::AddressSpace;
use crate::cap::channel::RecieveResult;
use crate::container::Arc;
use crate::event::{BroadcastEventEmitter, BroadcastEventListener};
use crate::sync::IMutex;
use super::kernel_stack::KernelStack;
use crate::container::Weak;
use crate::{make_id_type, prelude::*};

static NEXT_TID: AtomicUsize = AtomicUsize::new(0);

make_id_type!(ThreadId);

#[derive(Debug)]
pub struct Thread {
    name: String,
    pub tid: ThreadId,
    // this has to be atomic usize because it is written to in assembly
    pub rsp: AtomicUsize,
    kernel_stack: KernelStack,
    address_space: Arc<AddressSpace>,
}

impl Thread {
    pub fn new(
        name: String,
        kernel_stack: KernelStack,
        rsp: usize,
        address_space: Arc<AddressSpace>,
    ) -> Self {
        Thread {
            name,
            tid: NEXT_TID.fetch_add(1, Ordering::Relaxed).into(),
            rsp: AtomicUsize::new(rsp),
            kernel_stack,
            address_space,
        }
    }

    pub fn address_space(&self) -> &Arc<AddressSpace> {
        &self.address_space
    }

    /// This is the rsp value loaded when a syscall occurs for this thread
    pub fn syscall_rsp(&self) -> usize {
        self.kernel_stack.stack_top().as_usize()
    }

    pub fn is_current_thread(&self) -> bool {
        ptr::eq(
            self as *const Thread,
            Arc::as_ptr(&cpu_local_data().current_thread()),
        )
    }
}