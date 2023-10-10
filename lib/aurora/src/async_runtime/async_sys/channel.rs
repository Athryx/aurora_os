use core::pin::Pin;
use core::future::Future;
use core::task::{Context, Poll};

use futures::Stream;
use sys::{Channel, MessageBuffer, KResult, RecieveResult, MessageSent, EventId};
use bit_utils::Size;

use crate::async_runtime::EXECUTOR;
use crate::async_runtime::executor::{EventReciever, RecievedEvent, MessageRecievedEvent};
use crate::generate_async_wrapper;

pub struct AsyncChannel(Channel);

impl AsyncChannel {
    pub fn try_send(&self, buffer: &MessageBuffer) -> KResult<Size> {
        self.0.try_send(buffer)
    }

    pub fn try_recv(&self, buffer: &MessageBuffer) -> KResult<RecieveResult> {
        self.0.try_recv(buffer)
    }

    pub fn send<'a>(&'a self, buffer: MessageBuffer) -> impl Future<Output = KResult<Size>> + 'a {
        AsyncSend::Unpolled((&self.0, buffer))
    }

    pub fn recv<'a>(&'a self) -> impl Future<Output = KResult<MessageRecievedEvent>> + 'a {
        AsyncRecv::Unpolled(&self.0)
    }

    pub fn call<'a>(&'a self, buffer: MessageBuffer) -> impl Future<Output = KResult<MessageRecievedEvent>> + 'a {
        AsyncCall::Unpolled(&self.0, buffer)
    }

    pub fn recv_repeat<'a>(&'a self) -> impl Stream<Item = MessageRecievedEvent> + 'a {
        AsyncRecvRepeat::Unpolled(&self.0)
    }
}

impl From<Channel> for AsyncChannel {
    fn from(value: Channel) -> Self {
        AsyncChannel(value)
    }
}

generate_async_wrapper!(
    AsyncSend,
    (&'a Channel, MessageBuffer),
    Size,
    MessageSent,
    |data: (&Channel, MessageBuffer), event_pool, event_id| {
        data.0.async_send(&data.1, event_pool, event_id)
    },
    |event: MessageSent| event.recieved_size,
);

pub enum AsyncRecv<'a> {
    Unpolled(&'a Channel),
    Polled(EventReciever),
}

impl Future for AsyncRecv<'_> {
    type Output = KResult<MessageRecievedEvent>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match this {
            Self::Unpolled(channel) => {
                let event_reciever = EXECUTOR.with(|executor| {
                    let event_id = EventId::new();
                    channel.async_recv(executor.event_pool(), false, event_id)?;

                    let event_reciever = EventReciever::default();
                    executor.register_event_waiter_oneshot(event_id, cx.waker().clone(), event_reciever.clone());

                    Ok(event_reciever)
                })?;

                *this = Self::Polled(event_reciever);

                Poll::Pending
            },
            Self::Polled(event_reciever) => {
                match event_reciever.take_event() {
                    Some(RecievedEvent::MessageRecievedEvent(event)) => {
                        Poll::Ready(Ok(event))
                    },
                    None => Poll::Pending,
                    _ => panic!("invalid event recieved"),
                }
            },
        }
    }
}

impl Unpin for AsyncRecv<'_> {}

pub enum AsyncCall<'a> {
    Unpolled(&'a Channel, MessageBuffer),
    Polled(EventReciever),
}

impl Future for AsyncCall<'_> {
    type Output = KResult<MessageRecievedEvent>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match this {
            Self::Unpolled(channel, buffer) => {
                let event_reciever = EXECUTOR.with(|executor| {
                    let event_id = EventId::new();
                    channel.async_call(buffer, executor.event_pool(), event_id)?;

                    let event_reciever = EventReciever::default();
                    executor.register_event_waiter_oneshot(event_id, cx.waker().clone(), event_reciever.clone());

                    Ok(event_reciever)
                })?;

                *this = Self::Polled(event_reciever);

                Poll::Pending
            },
            Self::Polled(event_reciever) => {
                match event_reciever.take_event() {
                    Some(RecievedEvent::MessageRecievedEvent(event)) => {
                        Poll::Ready(Ok(event))
                    },
                    None => Poll::Pending,
                    _ => panic!("invalid event recieved"),
                }
            },
        }
    }
}

impl Unpin for AsyncCall<'_> {}

pub enum AsyncRecvRepeat<'a> {
    Unpolled(&'a Channel),
    Polled(EventId, EventReciever),
    Closed,
}

impl Stream for AsyncRecvRepeat<'_> {
    type Item = MessageRecievedEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        match this {
            Self::Unpolled(channel) => {
                let event_reciever: KResult<(EventId, EventReciever)> = EXECUTOR.with(|executor| {
                    let event_id = EventId::new();
                    channel.async_recv(executor.event_pool(), true, event_id)?;

                    let event_reciever = EventReciever::default();
                    executor.register_event_waiter_repeat(event_id, cx.waker().clone(), event_reciever.clone());

                    Ok((event_id, event_reciever))
                });

                match event_reciever {
                    Ok((event_id, event_reciever)) => *this = Self::Polled(event_id, event_reciever),
                    Err(_) => *this = Self::Closed,
                }

                Poll::Pending
            },
            Self::Polled(_, event_reciever) => {
                match event_reciever.take_event() {
                    Some(RecievedEvent::MessageRecievedEvent(event)) => {
                        Poll::Ready(Some(event))
                    },
                    None => Poll::Pending,
                    _ => panic!("invalid event recieved"),
                }
            },
            Self::Closed => Poll::Ready(None),
        }
    }
}

impl Drop for AsyncRecvRepeat<'_> {
    // TODO: stop event pool from waiting on event
    fn drop(&mut self) {
        if let Self::Polled(event_id, _) = self {
            EXECUTOR.with(|executor| {
                executor.remove_event_waiter(*event_id);
            });
        }
    }
}

impl Unpin for AsyncRecvRepeat<'_> {}