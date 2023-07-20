use sys::CapType;

use crate::prelude::*;
use super::CapObject;

#[derive(Debug)]
pub struct Channel {

}

impl Channel {
    pub fn new() -> Self {
        Channel {}
    }
}

impl CapObject for Channel {
    const TYPE: CapType = CapType::Channel;
}