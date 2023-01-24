use crate::prelude::*;

/// A kernel stack for a thread
#[derive(Debug)]
pub enum KernelStack {}

impl KernelStack {
    pub const DEFAULT_SIZE: usize = PAGE_SIZE * 16;
}