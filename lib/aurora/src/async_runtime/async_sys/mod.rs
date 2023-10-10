mod channel;
pub use channel::*;

#[macro_export]
macro_rules! generate_async_wrapper {
    ($name:ident, $data:ty, $return_type:ty, $event_type:ident, $action:expr, $get_return:expr,) => {
        pub enum $name<'a> {
            Unpolled($data),
            Polled($crate::async_runtime::executor::EventReciever),
        }
        
        impl core::future::Future for $name<'_> {
            type Output = sys::KResult<$return_type>;

            fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<Self::Output> {
                let this = self.get_mut();

                match this {
                    Self::Unpolled(data) => {
                        let event_reciever = $crate::async_runtime::EXECUTOR.with(|executor| {
                            let event_id = sys::EventId::new();
                            $action(*data, executor.event_pool(), event_id)?;

                            let event_reciever = $crate::async_runtime::executor::EventReciever::default();
                            executor.register_event_waiter_oneshot(event_id, cx.waker().clone(), event_reciever.clone());
        
                            Ok(event_reciever)
                        })?;

                        *this = Self::Polled(event_reciever);
        
                        core::task::Poll::Pending
                    },
                    Self::Polled(event_reciever) => {
                        match event_reciever.take_event() {
                            Some($crate::async_runtime::executor::RecievedEvent::OwnedEvent(sys::Event {
                                event_data: sys::EventData::$event_type(event),
                                ..
                            })) => {
                                core::task::Poll::Ready(Ok($get_return(event)))
                            },
                            None => core::task::Poll::Pending,
                            _ => panic!("invalid event recieved"),
                        }
                    },
                }
            }
        }
        
        impl Unpin for $name<'_> {}
    };
}