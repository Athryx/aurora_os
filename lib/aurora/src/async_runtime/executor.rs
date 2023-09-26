use core::future::Future;
use core::task::Poll;
use core::cell::RefCell;
use core::task::Waker;
use alloc::rc::Rc;
use alloc::sync::Arc;

use crossbeam_queue::SegQueue;
use sys::{EventPool, EventId, Event, CspaceTarget, CapFlags, cap_clone, EventParser, EventParseResult};
use bit_utils::Size;

use crate::allocator::addr_space::{MapEventPoolArgs, RegionPadding};
use crate::{prelude::*, this_context, addr_space};
use crate::collections::HashMap;
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

    pub fn spawn<T: 'static>(&self, task: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        let (task_handle, join_handle) = Task::new(task, self.task_queue.clone());

        let task_id = task_handle.id();
        self.tasks.borrow_mut().insert(task_id, task_handle);
        self.task_queue.push(task_id);

        join_handle
    }

    pub fn register_event_waiter(
        &self,
        event_id: EventId,
        waker: Waker,
        recieved_event: Rc<RefCell<RecievedEvent>>,
    ) {
        self.event_waiters.borrow_mut().insert(
            event_id,
            EventWaiter {
                waker,
                recieved_event,
            },
        );
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
            let Some(waiter) = event_waiters.remove(&event.event_id()) else {
                continue;
            };

            match event {
                EventParseResult::Event(event) => {
                    *waiter.recieved_event.borrow_mut() = RecievedEvent::OwnedEvent(event);
                },
                EventParseResult::MessageRecieved(message_event) => {
                    *waiter.recieved_event.borrow_mut() = RecievedEvent::MessageRecievedEvent(MessageRecievedEvent {
                        data: message_event.message_data.as_ptr(),
                        len: message_event.message_data.len(),
                    });
                },
            }

            waiter.waker.wake();
        }

        Ok(())
    }
}

impl !Send for Executor {}

/// Something that is waiting on an event
struct EventWaiter {
    waker: Waker,
    recieved_event: Rc<RefCell<RecievedEvent>>,
}

pub struct MessageRecievedEvent {
    data: *const u8,
    len: usize,
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

pub enum RecievedEvent {
    OwnedEvent(Event),
    MessageRecievedEvent(MessageRecievedEvent),
}