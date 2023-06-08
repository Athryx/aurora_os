use serde::{Serialize, Deserialize};
use sys::{Process, Allocator, Spawner};

#[derive(Debug, Serialize, Deserialize)]
pub struct Context {
    pub process: Process,
    pub allocator: Allocator,
    pub spawner: Spawner,
}