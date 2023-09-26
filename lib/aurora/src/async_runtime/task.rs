use core::future::Future;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU64, Ordering};
use core::pin::Pin;
use core::task::{Waker, Poll, Context};
use core::cell::RefCell;
use core::any::Any;
use alloc::rc::Rc;
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

#[derive(Clone)]
pub struct TaskHandle(Rc<RefCell<Task>>);

impl TaskHandle {
    pub fn id(&self) -> TaskId {
        self.0.borrow().id
    }

    pub fn poll(&self) -> Poll<()> {
        self.0.borrow_mut().poll()
    }
}

pub struct Task {
    id: TaskId,
    future: Pin<Box<dyn Future<Output = Box<dyn Any>>>>,
    waker: Waker,
    task_join: Rc<RefCell<TaskJoinInner>>,
}

impl Task {
    pub fn new<T: 'static>(task: impl Future<Output = T> + 'static, task_queue: Arc<SegQueue<TaskId>>) -> (TaskHandle, JoinHandle<T>) {
        // make a future that wraps the original future's return value in a Box<dyn Any>
        let future = async {
            let task_result = task.await;
            Box::new(task_result) as Box<dyn Any>
        };

        let task_id = TaskId::new();

        let task = Task {
            id: task_id,
            future: Box::pin(future),
            waker: Arc::new(TaskWaker {
                task_id,
                task_queue,
            }).into(),
            task_join: Rc::default(),
        };

        let join_handle = JoinHandle {
            inner: task.task_join.clone(),
            _marker: PhantomData,
        };

        let task_handle = TaskHandle(Rc::new(RefCell::new(task)));

        (task_handle, join_handle)
    }

    pub fn poll(&mut self) -> Poll<()> {
        let Task {
            future,
            waker,
            ..
        } = self;

        let mut context = Context::from_waker(&waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => {
                let mut task_join = self.task_join.borrow_mut();

                task_join.is_finished = true;
                task_join.value = Some(result);
                task_join.waiting_waker.take().map(Waker::wake);

                Poll::Ready(())
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Default)]
pub(super) struct TaskJoinInner {
    is_finished: bool,
    pub(super) value: Option<Box<dyn Any>>,
    waiting_waker: Option<Waker>,
}

pub struct JoinHandle<T> {
    pub(super) inner: Rc<RefCell<TaskJoinInner>>,
    _marker: PhantomData<Box<T>>,
}

impl<T: 'static> JoinHandle<T> {
    pub fn is_finished(&self) -> bool {
        self.inner.borrow().is_finished
    }

    /// Gets the output of this join handle
    /// 
    /// # Panics
    /// 
    /// panics if the task has not finished
    pub(super) fn get_output(self) -> T {
        *self.inner.borrow_mut().value.take().unwrap().downcast().unwrap()
    }
}

impl<T: 'static> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut inner = self.inner.borrow_mut();

        let Some(value) = inner.value.take() else {
            assert!(inner.waiting_waker.is_none(), "multiple tasks cannot wait on 1 task");
            inner.waiting_waker = Some(cx.waker().clone());
            return Poll::Pending;
        };

        let out = value.downcast::<T>()
            .expect("task returned wrong type of value");

        Poll::Ready(*out)
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