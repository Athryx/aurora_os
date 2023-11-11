use sys::{ThreadGroup, AddressSpace, CapabilitySpace, Allocator};

#[derive(Debug)]
pub struct Context {
    pub thread_group: ThreadGroup,
    pub address_space: AddressSpace,
    pub capability_space: CapabilitySpace,
    pub allocator: Allocator,
}