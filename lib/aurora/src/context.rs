use sys::{Process, Allocator, Spawner};

#[derive(Debug)]
pub struct Context {
    pub process: Process,
    pub allocator: Allocator,
    pub spawner: Spawner,
}