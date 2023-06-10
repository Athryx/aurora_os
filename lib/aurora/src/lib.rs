#![no_std]

#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_take)]

extern crate alloc;

use bit_utils::Size;
use sys::{CapId, Process, Allocator, Spawner, Memory};
use thiserror_no_std::Error;

use addr_space_manager::{AddrSpaceManager, AddrSpaceError, RegionPadding, MappedRegion};
use context::Context;
use sync::{Once, Mutex, MutexGuard};

mod addr_space_manager;
mod allocator;
mod context;
pub mod debug_print;
mod prelude;
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
    #[error("Unexpected end of process data")]
    ProcessDataEnd,
    #[error("Invalid capability id in the process data")]
    InvalidCapId,
    #[error("Error initilizing address space: {0}")]
    AdrSpaceError(#[from] AddrSpaceError),
}

/// Performs all the initilization required for memory mapping and allocation to work
/// 
/// Format of process data is as follows:
/// word 0: process cap id
/// word 1: allocator cap id
/// word 2: spawner cap id
/// word 3, 4, 5, 6: aslr seed
/// 
/// The next several words are repeated for every memory region
/// word n: memory cap id
/// word n + 1: memory map address
/// word n + 2: memory map size (bytes)
/// word n + 3: padding start size (bytes)
/// word n + 4: padding start size (bytes)
pub fn init_allocation(mut process_data: &[usize]) -> Result<(), InitError> {
    let mut take = || {
        process_data.take_first().copied().ok_or(InitError::ProcessDataEnd)
    };

    // Initialize context from first 3 words of process data
    let process_id = CapId::try_from(take()?).ok_or(InitError::InvalidCapId)?;
    let allocator_id = CapId::try_from(take()?).ok_or(InitError::InvalidCapId)?;
    let spawner_id = CapId::try_from(take()?).ok_or(InitError::InvalidCapId)?;

    let process = Process::try_from(process_id).ok_or(InitError::InvalidCapId)?;
    let allocator = Allocator::try_from(allocator_id).ok_or(InitError::InvalidCapId)?;
    let spawner = Spawner::try_from(spawner_id).ok_or(InitError::InvalidCapId)?;

    let context = Context {
        process,
        allocator,
        spawner,
    };

    THIS_CONTEXT.call_once(|| context);


    // initialize address space
    let mut aslr_seed = [0; 32];
    aslr_seed[0..8].copy_from_slice(&take()?.to_le_bytes());
    aslr_seed[8..16].copy_from_slice(&take()?.to_le_bytes());
    aslr_seed[16..24].copy_from_slice(&take()?.to_le_bytes());
    aslr_seed[24..32].copy_from_slice(&take()?.to_le_bytes());

    let mut addr_space = AddrSpaceManager::new(aslr_seed)?;

    while process_data.len() != 0 {
        // redefine take to avoid ownership issues
        let mut take = || {
            process_data.take_first().copied().ok_or(InitError::ProcessDataEnd)
        };

        let memory_id = CapId::try_from(take()?).ok_or(InitError::InvalidCapId)?;
        let memory = Memory::try_from(memory_id).ok_or(InitError::InvalidCapId)?;

        let map_address = take()?;
        let map_size = Size::from_bytes(take()?);

        let padding = RegionPadding {
            start: Size::from_bytes(take()?),
            end: Size::from_bytes(take()?),
        };

        let region = MappedRegion {
            memory_cap: Some(memory),
            address: map_address,
            size: map_size,
            padding,
        };

        // TODO: add more checks to make sure this doesn't overlap
        addr_space.insert_region(region)?;
    }

    ADDR_SPACE.call_once(|| Mutex::new(addr_space));

    Ok(())
}