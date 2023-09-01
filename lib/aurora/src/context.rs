use serde::{Serialize, Deserialize};
use sys::{Process, Allocator, Spawner};

use crate::this_context;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub process: Process,
    pub allocator: Allocator,
    pub spawner: Spawner,
}

impl Context {
    pub fn is_current_process(&self) -> bool {
        this_context().process == self.process
    }
}