use core::future::Future;
use core::task::Poll;
use core::cell::RefCell;
use core::task::Waker;
use alloc::rc::Rc;
use alloc::sync::Arc;

use crossbeam_queue::SegQueue;
use sys::{EventPool, Reply, EventId, Event, CspaceTarget, CapFlags, cap_clone, EventParser, EventParseResult};
use bit_utils::Size;
use aurora_core::allocator::addr_space::{MapEventPoolArgs, RegionPadding};
use aurora_core::{prelude::*, this_context, addr_space};
use aurora_core::collections::HashMap;

use super::AsyncError;
use super::task::{TaskId, Task, JoinHandle, TaskHandle};

const ASYNC_EVENT_POOL_MAX_SIZE: Size = Size::from_pages(1000);

pub struct Executor {
    tasks: RefCell<HashMap<TaskId, TaskHandle>>,
    /// A queue of tasks that are ready to be run
    task_queue: Arc<SegQueue<TaskId>>,
    /// Event pool used by this executor
    event_pool: EventPool,
    /// Tasks which are waiting on an event
    event_waiters: RefCell<HashMap<EventId, EventWaiter>>,
}

impl Executor {
    pub fn new() -> Result<Self, AsyncError> {
        let event_pool = EventPool::new(&this_context().allocator, ASYNC_EVENT_POOL_MAX_SIZE)?;
        let cloned_event_pool = cap_clone(CspaceTarget::Current, CspaceTarget::Current, &event_pool, CapFlags::all())?;

        addr_space().map_event_pool(MapEventPoolArgs {
            event_pool: cloned_event_pool,
            address: None,
            padding: RegionPadding::default(),
        })?;

        Ok(Executor {
            tasks: RefCell::new(HashMap::default()),
            task_queue: Arc::new(SegQueue::new()),
            event_pool,
            event_waiters: RefCell::new(HashMap::default()),
        })
    }

    pub fn event_pool(&self) -> &EventPool {
        &self.event_pool
    }

    pub fn spawn<T: 'static>(&self, task: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        let (task_handle, join_handle) = Task::new(task, self.task_queue.clone());

        let task_id = task_handle.id();
        self.tasks.borrow_mut().insert(task_id, task_handle);
        self.task_queue.push(task_id);

        join_handle
    }

    pub fn register_event_waiter_oneshot(
        &self,
        event_id: EventId,
        waker: Waker,
        event_reciever: EventReciever,
    ) {
        self.event_waiters.borrow_mut().insert(
            event_id,
            EventWaiter {
                waker,
                event_reciever,
                oneshot: true,
            },
        );
    }

    pub fn register_event_waiter_repeat(
        &self,
        event_id: EventId,
        waker: Waker,
        event_reciever: EventReciever,
    ) {
        self.event_waiters.borrow_mut().insert(
            event_id,
            EventWaiter {
                waker,
                event_reciever,
                oneshot: false,
            },
        );
    }

    pub fn remove_event_waiter(&self, event_id: EventId) {
        self.event_waiters.borrow_mut().remove(&event_id);
    }

    /// Runs all the tasks in this executor, returns on error or when the last task has completed
    pub fn run(&self) -> Result<(), AsyncError> {
        loop {
            self.run_ready_tasks();
            if self.tasks.borrow().len() == 0 {
                return Ok(());
            }

            self.await_event()?;
        }
    }

    fn run_ready_tasks(&self) {
        while let Some(task_id) = self.task_queue.pop() {
            let task = self.tasks.borrow().get(&task_id)
                .expect("task id found in ready queue but no task with given id exists")
                .clone();

            if let Poll::Ready(()) = task.poll() {
                self.tasks.borrow_mut().remove(&task_id);
            }
        }
    }

    /// Blocks the calling thread until any events arrive, and wakes any tasks waiting for those events
    pub fn await_event(&self) -> Result<(), AsyncError> {
        let event_data = self.event_pool.await_event(None)?;
        let mut event_waiters = self.event_waiters.borrow_mut();

        // safety: async context is non send so no one is calling event_data::as_slice at the same time
        let event_parser = EventParser::new(unsafe { event_data.as_slice() });

        for event in event_parser {
            let event_id = event.event_id();
            let Some(waiter) = event_waiters.get(&event_id) else {
                continue;
            };

            match event {
                EventParseResult::Event(event) => {
                    *waiter.event_reciever.0.borrow_mut() = Some(RecievedEvent::OwnedEvent(event));
                },
                EventParseResult::MessageRecieved(mut message_event) => {
                    *waiter.event_reciever.0.borrow_mut() = Some(RecievedEvent::MessageRecievedEvent(MessageRecievedEvent {
                        data: message_event.message_data.as_ptr(),
                        len: message_event.message_data.len(),
                        reply: message_event.reply.take(),
                    }));
                },
            }

            waiter.waker.wake_by_ref();

            if waiter.oneshot {
                event_waiters.remove(&event_id);
            }
        }

        Ok(())
    }
}

impl !Send for Executor {}

/// Something that is waiting on an event
#[derive(Debug)]
struct EventWaiter {
    waker: Waker,
    event_reciever: EventReciever,
    // if it is oneshot, it will be removed on next event
    oneshot: bool,
}

#[derive(Debug)]
pub struct MessageRecievedEvent {
    data: *const u8,
    len: usize,
    pub reply: Option<Reply>,
}

impl MessageRecievedEvent {
    /// # Safety
    /// 
    /// This must not be called after the event range for the current event pool is invalidated (when `await_event` is called again)
    pub unsafe fn as_slice(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(self.data, self.len)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EventReciever(Rc<RefCell<Option<RecievedEvent>>>);

impl EventReciever {
    pub fn take_event(&self) -> Option<RecievedEvent> {
        self.0.borrow_mut().take()
    }
}

#[derive(Debug)]
pub enum RecievedEvent {
    OwnedEvent(Event),
    MessageRecievedEvent(MessageRecievedEvent),
}