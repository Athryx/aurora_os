use core::mem::size_of;

use bytemuck::{Pod, Zeroable, AnyBitPattern, try_from_bytes};
use bit_utils::align_of;

use crate::CapId;

macro_rules! create_event_types {
    ($( $events:ident ),*,) => {
        #[repr(usize)]
        enum EventNums {
            $(
                $events,
            )*
            // this one is special because it is variably sized
            MessageRecieved,
            Invalid,
        }

        impl EventNums {
            fn from_id(n: usize) -> Option<EventNums> {
                if n >= Self::Invalid as usize {
                    None
                } else {
                    unsafe {
                        core::mem::transmute(n)
                    }
                }
            }

            fn event_size(&self) -> usize {
                match self {
                    $(
                        Self::$events => size_of::<$events>() + size_of::<Self>(),
                    )*
                    Self::MessageRecieved => panic!("message recieved is unsized"),
                    Self::Invalid => panic!("invalid event num"),
                }
            }
        }

        #[derive(Debug, Clone, Copy)]
        pub enum Event {
            $(
                $events($events),
            )*
        }

        impl Event {
            pub fn as_raw(&self) -> EventRaw {
                match self {
                    $(
                        Self::$events(event) => EventRaw {
                            tag: EventNums::$events,
                            inner: EventRawInner {
                                $events: *event,
                            },
                        },
                    )*
                }
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

        #[derive(Debug, Clone, Copy)]
        pub struct MessageRecievedEvent<'a> {
            pub channel_id: CapId,
            pub message_data: &'a [u8],
        }

        pub enum EventParseResult<'a> {
            MessageRecieved(MessageRecievedEvent<'a>),
            Event(Event),
        }

        impl<'a> Iterator for EventParser<'a> {
            type Item = EventParseResult<'a>;

            fn next(&mut self) -> Option<Self::Item> {
                self.assert_aligned();

                let event_type = EventNums::from_id(self.take()?)?;
        
                match event_type {
                    $(
                        EventNums::$events => Some(EventParseResult::Event(Event::$events(self.take()?))),
                    )*
                    EventNums::MessageRecieved => {
                        let channel_id = self.take()?;
                        let message_size = self.take()?;

                        let channel_id = CapId::try_from(channel_id)?;
                        let message_data = self.take_bytes(message_size)?;

                        Some(EventParseResult::MessageRecieved(MessageRecievedEvent {
                            channel_id,
                            message_data,
                        }))
                    },
                    EventNums::Invalid => unreachable!(),
                }
            }
        }

        #[repr(C)]
        pub struct EventRaw {
            tag: EventNums,
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

create_event_types! {
    MessageSent,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MessageSent {
    pub channel_id: usize,
    pub message_buffer_id: usize,
    pub message_buffer_offset: usize,
    pub message_buffer_len: usize,
}