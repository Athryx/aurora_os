use core::mem::size_of;
use core::sync::atomic::{AtomicU64, Ordering};

use bytemuck::{Pod, Zeroable, AnyBitPattern, try_from_bytes};
use bit_utils::align_of;
use strum::FromRepr;

use crate::{CapId, Reply};

/// The event number of message recieved, kernel needs to know this
pub const MESSAGE_RECIEVED_NUM: usize = EventNums::MessageRecieved as usize;

macro_rules! create_event_types {
    ($( $events:ident ),*,) => {
        #[repr(usize)]
        #[derive(FromRepr)]
        enum EventNums {
            $(
                $events,
            )*
            // this one is special because it is variably sized
            MessageRecieved,
        }

        impl EventNums {
            fn event_size(&self) -> usize {
                match self {
                    $(
                        Self::$events => size_of::<$events>() + size_of::<EventId>() + size_of::<Self>(),
                    )*
                    Self::MessageRecieved => panic!("message recieved is unsized"),
                }
            }
        }

        #[derive(Debug, Clone, Copy)]
        pub enum EventData {
            $(
                $events($events),
            )*
        }

        #[derive(Debug, Clone, Copy)]
        pub struct Event {
            pub event_data: EventData,
            pub event_id: EventId,
        }

        impl Event {
            pub fn as_raw(&self) -> EventRaw {
                match self.event_data {
                    $(
                        EventData::$events(event) => EventRaw {
                            tag: EventNums::$events,
                            event_id: self.event_id,
                            inner: EventRawInner {
                                $events: event,
                            },
                        },
                    )*
                }
            }

            pub fn event_id(&self) -> EventId {
                self.event_id
            }
        }

        pub struct EventParser<'a> {
            event_data: &'a [u8],
        }
        
        impl<'a> EventParser<'a> {
            pub fn new(event_data: &'a [u8]) -> Self {
                let out = EventParser {
                    event_data,
                };

                out.assert_aligned();
                out
            }

            fn assert_aligned(&self) {
                assert!(align_of(self.event_data.as_ptr() as usize) == size_of::<usize>());
                assert!(self.event_data.len() % size_of::<usize>() == 0);
            }

            fn take_bytes(&mut self, num_bytes: usize) -> Option<&'a [u8]> {
                if num_bytes > self.event_data.len() {
                    None
                } else {
                    let data = &self.event_data[..num_bytes];
                    self.event_data = &self.event_data[num_bytes..];

                    Some(data)
                }
            }
        
            fn take<T: AnyBitPattern + Copy>(&mut self) -> Option<T> {
                let data = self.take_bytes(size_of::<T>())?;

                let out = try_from_bytes(data).ok()?;
                Some(*out)
            }
        }

        #[derive(Debug)]
        pub struct MessageRecievedEvent<'a> {
            pub event_id: EventId,
            pub reply: Option<Reply>,
            pub message_data: &'a [u8],
        }

        pub enum EventParseResult<'a> {
            MessageRecieved(MessageRecievedEvent<'a>),
            Event(Event),
        }

        impl EventParseResult<'_> {
            pub fn event_id(&self) -> EventId {
                match self {
                    Self::MessageRecieved(message_event) => message_event.event_id,
                    Self::Event(event) => event.event_id(),
                }
            }
        }

        impl<'a> Iterator for EventParser<'a> {
            type Item = EventParseResult<'a>;

            fn next(&mut self) -> Option<Self::Item> {
                self.assert_aligned();

                let event_type = EventNums::from_repr(self.take()?)?;
                let event_id = EventId(self.take()?);

                match event_type {
                    $(
                        EventNums::$events => {
                            let event_data = EventData::$events(self.take()?);
                            let event = Event {
                                event_data,
                                event_id,
                            };

                            Some(EventParseResult::Event(event))
                        },
                    )*
                    EventNums::MessageRecieved => {
                        let reply_id = self.take()?;
                        let reply = CapId::try_from(reply_id)
                            .map(Reply::from_cap_id)
                            .flatten();

                        let message_size = self.take()?;

                        let message_data = self.take_bytes(message_size)?;

                        Some(EventParseResult::MessageRecieved(MessageRecievedEvent {
                            event_id,
                            reply,
                            message_data,
                        }))
                    },
                }
            }
        }

        #[repr(C)]
        pub struct EventRaw {
            tag: EventNums,
            event_id: EventId,
            inner: EventRawInner,
        }

        impl EventRaw {
            pub fn as_bytes(&self) -> &[u8] {
                let ptr = self as *const Self as *const u8;

                unsafe {
                    core::slice::from_raw_parts(ptr, self.tag.event_size())
                }
            }
        }

        union EventRawInner {
            $(
                $events: $events,
            )*
        }
    };
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Pod, Zeroable)]
pub struct EventId(u64);

impl EventId {
    pub fn new() -> EventId {
        static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(0);

        EventId(NEXT_EVENT_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn from_u64(n: u64) -> Self {
        EventId(n)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

create_event_types! {
    MessageSent,
    ThreadExit,
}

pub trait EventSyncReturn {
    type SyncReturn;

    fn as_sync_return(&self) -> Self::SyncReturn;
    fn from_sync_return(data: Self::SyncReturn) -> Self;
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MessageSent {
    pub message_buffer_id: usize,
    pub message_buffer_offset: usize,
    pub message_buffer_len: usize,
}

impl EventSyncReturn for MessageSent {
    type SyncReturn = (usize, usize, usize);

    fn as_sync_return(&self) -> Self::SyncReturn {
        (
            self.message_buffer_id,
            self.message_buffer_offset,
            self.message_buffer_len
        )
    }

    fn from_sync_return(data: Self::SyncReturn) -> Self {
        MessageSent {
            message_buffer_id: data.0,
            message_buffer_offset: data.1,
            message_buffer_len: data.2,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ThreadExit;

impl EventSyncReturn for ThreadExit {
    type SyncReturn = ();

    fn as_sync_return(&self) -> Self::SyncReturn {
        ()
    }

    fn from_sync_return(_: Self::SyncReturn) -> Self {
        ThreadExit
    }
}