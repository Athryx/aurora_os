#![no_std]

#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(slice_ptr_get)]
#![feature(slice_take)]

extern crate alloc;

use aser::{from_bytes, AserError};
use bit_utils::Size;
use sys::{CapId, Process, Allocator, Spawner, Memory};
pub use sys::{ProcessInitData, ProcessMemoryEntry, process_data_from_slice};
use thiserror_no_std::Error;

use allocator::addr_space::{AddrSpaceManager, AddrSpaceError, RegionPadding, MappedRegion};
use context::Context;
use sync::{Once, Mutex, MutexGuard};
use env::{Namespace, THIS_NAMESPACE};

mod allocator;
mod context;
pub mod collections;
pub mod debug_print;
pub mod env;
mod prelude;
pub mod process;
mod sync;

static THIS_CONTEXT: Once<Context> = Once::new();

pub fn this_context() -> &'static Context {
    THIS_CONTEXT.get().unwrap()
}

static ADDR_SPACE: Once<Mutex<AddrSpaceManager>> = Once::new();

pub fn addr_space() -> MutexGuard<'static, AddrSpaceManager> {
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
        let process_id = CapId::try_from(value.process_cap_id).ok_or(InitError::InvalidCapId)?;
        let allocator_id = CapId::try_from(value.allocator_cap_id).ok_or(InitError::InvalidCapId)?;
        let spawner_id = CapId::try_from(value.spawner_cap_id).ok_or(InitError::InvalidCapId)?;

        let process = Process::try_from(process_id).ok_or(InitError::InvalidCapId)?;
        let allocator = Allocator::try_from(allocator_id).ok_or(InitError::InvalidCapId)?;
        let spawner = Spawner::try_from(spawner_id).ok_or(InitError::InvalidCapId)?;

        Ok(Context {
            process,
            allocator,
            spawner,
        })
    }
}

impl TryFrom<ProcessMemoryEntry> for MappedRegion {
    type Error = InitError;

    fn try_from(value: ProcessMemoryEntry) -> Result<Self, Self::Error> {
        let memory_id = CapId::try_from(value.memory_cap_id).ok_or(InitError::InvalidCapId)?;
        let memory = Memory::try_from(memory_id).ok_or(InitError::InvalidCapId)?;

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

    let mut addr_space = AddrSpaceManager::new(init_data.aslr_seed)?;
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