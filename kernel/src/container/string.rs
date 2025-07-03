use core::fmt::{self, Write};
use core::ops::{Deref, DerefMut};
use core::str;

use crate::mem::HeapRef;
use crate::prelude::*;

pub struct String {
    data: Vec<u8>,
}

impl String {
    pub fn new(allocer: HeapRef) -> String {
        String {
            data: Vec::new(allocer),
        }
    }

    pub unsafe fn from_utf8_unchecked(data: Vec<u8>) -> String {
        String {
            data,
        }
    }

    pub fn from_str(allocer: HeapRef, str: &str) -> KResult<String> {
        Ok(String {
            data: Vec::from_slice(allocer, str.as_bytes())?,
        })
    }

    pub fn push_str(&mut self, str: &str) -> KResult<()> {
        self.data.extend_from_slice(str.as_bytes())
    }
}

impl Deref for String {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        unsafe { str::from_utf8_unchecked(&self.data) }
    }
}

impl DerefMut for String {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { str::from_utf8_unchecked_mut(&mut self.data) }
    }
}

impl fmt::Debug for String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl fmt::Display for String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl Write for String {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s).or(Err(fmt::Error))
    }
}

#[macro_export]
macro_rules! format {
    ($allocator:expr, $($args:tt)*) => {{
        let mut string = String::new($allocator);
        string.write_fmt(core::format_args!($($args)*))
            .expect("a formatting trait implementation returnad an error");
        string
    }}
}