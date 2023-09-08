#![no_std]

#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(slice_ptr_get)]
#![feature(slice_take)]

extern crate alloc;

use aser::{from_bytes, AserError};
use bit_utils::Size;
use sys::{CapId, ThreadGroup, Allocator, Memory, AddressSpace, CapabilitySpace};
pub use sys::{ProcessInitData, ProcessMemoryEntry, Capability, process_data_from_slice};
use thiserror_no_std::Error;

use allocator::addr_space::{LocalAddrSpaceManager, AddrSpaceError, RegionPadding, MappedRegion};
use context::Context;
use sync::{Once, Mutex, MutexGuard};
use env::{Namespace, THIS_NAMESPACE};

mod allocator;
mod context;
pub mod collections;
pub mod debug_print;
pub mod env;
pub mod prelude;
pub mod process;
mod sync;

static THIS_CONTEXT: Once<Context> = Once::new();

pub fn this_context() -> &'static Context {
    THIS_CONTEXT.get().unwrap()
}

static ADDR_SPACE: Once<Mutex<LocalAddrSpaceManager>> = Once::new();

pub fn addr_space() -> MutexGuard<'static, LocalAddrSpaceManager> {
    ADDR_SPACE.get().unwrap().lock()
}

#[derive(Debug, Error)]
pub enum InitError {
    #[error("Invalid capability id in the process data")]
    InvalidCapId,
    #[error("Error initilizing address space: {0}")]
    AdrSpaceError(#[from] AddrSpaceError),
    #[error("Error deserializing namespace data: {0}")]
    SerializationError(#[from] AserError),
}

impl TryFrom<ProcessInitData> for Context {
    type Error = InitError;

    fn try_from(value: ProcessInitData) -> Result<Self, Self::Error> {
        let thread_group_id = CapId::try_from(value.thread_group_id).ok_or(InitError::InvalidCapId)?;
        let address_space_id = CapId::try_from(value.address_space_id).ok_or(InitError::InvalidCapId)?;
        let capability_space_id = CapId::try_from(value.capability_space_id).ok_or(InitError::InvalidCapId)?;
        let allocator_id = CapId::try_from(value.allocator_id).ok_or(InitError::InvalidCapId)?;

        let thread_group = ThreadGroup::from_cap_id(thread_group_id).ok_or(InitError::InvalidCapId)?;
        let address_space = AddressSpace::from_cap_id(address_space_id).ok_or(InitError::InvalidCapId)?;
        let capability_space = CapabilitySpace::from_cap_id(capability_space_id).ok_or(InitError::InvalidCapId)?;
        let allocator = Allocator::from_cap_id(allocator_id).ok_or(InitError::InvalidCapId)?;

        Ok(Context {
            thread_group,
            address_space,
            capability_space,
            allocator,
        })
    }
}

impl TryFrom<ProcessMemoryEntry> for MappedRegion {
    type Error = InitError;

    fn try_from(value: ProcessMemoryEntry) -> Result<Self, Self::Error> {
        let memory_id = CapId::try_from(value.memory_cap_id).ok_or(InitError::InvalidCapId)?;
        let memory = Memory::from_cap_id(memory_id).ok_or(InitError::InvalidCapId)?;

        let padding = RegionPadding {
            start: Size::from_bytes(value.padding_start),
            end: Size::from_bytes(value.padding_end),
        };

        Ok(MappedRegion {
            memory_cap: Some(memory),
            address: value.map_address,
            size: Size::from_bytes(value.map_size),
            padding,
        })
    }
}

/// Performs all the initilization required for memory mapping and allocation to work
pub fn init_allocation(init_data: ProcessInitData, memory_entries: &[ProcessMemoryEntry]) -> Result<(), InitError> {
    let context = init_data.try_into()?;
    THIS_CONTEXT.call_once(|| context);

    let mut addr_space = LocalAddrSpaceManager::new_local(init_data.aslr_seed)?;
    for memory_entry in memory_entries {
        let region = (*memory_entry).try_into()?;

        // TODO: add more checks to make sure regions don't overlap
        addr_space.insert_region(region)?;
    }

    ADDR_SPACE.call_once(|| Mutex::new(addr_space));

    Ok(())
}

/// Initializes the rest of the aurora library
/// 
/// Requires [`init_allocation`] to be called first
pub fn init(namespace_data: &[u8]) -> Result<(), InitError> {
    let namespace: Namespace = from_bytes(namespace_data)?;
    THIS_NAMESPACE.call_once(|| namespace);
    Ok(())
}