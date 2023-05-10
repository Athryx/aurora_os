use core::sync::atomic::{AtomicUsize, AtomicPtr, Ordering};

use crate::container::Arc;
use crate::mem::MemOwner;
use super::kernel_stack::KernelStack;
use crate::container::{ListNode, ListNodeData, Weak};
use crate::process::Process;
use crate::prelude::*;

pub use sys::Tid;

#[derive(Debug)]
pub struct Thread {
    pub tid: Tid,
    name: String,
    // FIXME: maybe handle setting this to null when thread handle dropped (see if it is an issue)
    handle: AtomicPtr<ThreadHandle>,
    pub process: Weak<Process>,
    // this has to be atomic usize because it is written to in assembly
    pub rsp: AtomicUsize,
    // if this is non zero, the scheduler will exchange this field with 0 when switching away from a suspend state,
    // if waiting_capid is already 0, the scheduler knows some other code is already switching the task to ready
    pub waiting_capid: AtomicUsize,
    kernel_stack: KernelStack,
}

impl Thread {
    /// Creates a new thread, and returns the thread and its thread handle
    /// 
    /// If `kernel_stack` is owned, it must use the same allocator as the process (the drop implementation assumes this to be true)
    pub fn new(
        tid: Tid,
        name: String,
        process: Weak<Process>,
        kernel_stack: KernelStack,
        rsp: usize
    ) -> KResult<(Arc<Thread>, MemOwner<ThreadHandle>)> {
        let allocer = process.alloc_ref();

        let thread = Arc::new(Thread {
            tid,
            name,
            handle: AtomicPtr::new(null_mut()),
            process,
            rsp: AtomicUsize::new(rsp),
            waiting_capid: AtomicUsize::new(0),
            kernel_stack,
        }, allocer)?;

        let thread_handle = ThreadHandle::new(thread.clone())?;

        thread.handle.store(thread_handle.ptr_mut(), Ordering::Release);

        Ok((thread, thread_handle))
    }

    /// This is the rsp value loaded when a syscall occurs for this thread
    pub fn syscall_rsp(&self) -> usize {
        self.kernel_stack.stack_top().as_usize()
    }
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

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

/// The `ThreadHandle` references a [`Thread`] and is used in the scheduler to schedule threads
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
            &mut allocator,
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
            thread_handle.drop_in_place(&mut allocator);
        }
    }
}

impl ListNode for ThreadHandle {
    fn list_node_data(&self) -> &ListNodeData<Self> {
        &self.list_node_data
    }

    fn list_node_data_mut(&mut self) -> &mut ListNodeData<Self> {
        &mut self.list_node_data
    }
}
