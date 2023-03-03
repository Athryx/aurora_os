use core::fmt;
use core::ops::{Deref, DerefMut};
use core::str;

use crate::alloc::AllocRef;
use crate::prelude::*;

pub struct String {
    data: Vec<u8>,
}

impl String {
    fn new(allocer: AllocRef) -> String {
        String {
            data: Vec::new(allocer),
        }
    }

    unsafe fn from_utf8_unchecked(data: Vec<u8>) -> String {
        String {
            data,
        }
    }

    fn from_str(allocer: AllocRef, str: &str) -> KResult<String> {
        Ok(String {
            data: Vec::from_slice(allocer, str.as_bytes())?,
        })
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