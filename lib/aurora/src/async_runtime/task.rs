use core::future::Future;
use core::sync::atomic::{AtomicU64, Ordering};
use core::pin::Pin;
use core::task::{Waker, Poll, Context};
use alloc::sync::Arc;
use alloc::task::Wake;

use crossbeam_queue::SegQueue;

use crate::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

impl TaskId {
    pub fn new() -> TaskId {
        static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(0);

        TaskId(NEXT_TASK_ID.load(Ordering::Relaxed))
    }
}

pub struct Task {
    pub(super) id: TaskId,
    pub(super) future: Pin<Box<dyn Future<Output = ()>>>,
    pub(super) waker: Waker,
}

impl Task {
    pub fn new(future: Pin<Box<dyn Future<Output = ()>>>, task_queue: Arc<SegQueue<TaskId>>) -> Self {
        let task_id = TaskId::new();
        Task {
            id: task_id,
            future,
            waker: Arc::new(TaskWaker {
                task_id,
                task_queue,
            }).into(),
        }
    }

    pub fn poll(&mut self) -> Poll<()> {
        let Task {
            future,
            waker,
            ..
        } = self;

        let mut context = Context::from_waker(&waker);
        future.as_mut().poll(&mut context)
    }
}

struct TaskWaker {
    task_id: TaskId,
    task_queue: Arc<SegQueue<TaskId>>,
}

impl TaskWaker {
    fn wake_task(&self) {
        self.task_queue.push(self.task_id);
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}