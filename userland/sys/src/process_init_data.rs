//! Structure definitions for data passed into `aurora::init_allocator`
//! 
//! Thes definitions need to be heare because the kernel
//! needs to know them to start the first userspace process

use core::mem::size_of;

use bytemuck::{Pod, Zeroable, PodCastError, try_from_bytes, try_cast_slice};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct StackInfo {
    pub process_data_address: usize,
    pub process_data_size: usize,
    pub namespace_data_address: usize,
    pub namespace_data_size: usize,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ProcessInitData {
    pub thread_group_id: usize,
    pub address_space_id: usize,
    pub capability_space_id: usize,
    pub allocator_id: usize,
    pub main_thread_id: usize,
    pub stack_region_start_address: usize,
    pub aslr_seed: [u8; 32]
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ProcessMemoryEntry {
    pub memory_cap_id: usize,
    /// Memory size in bytes
    /// 
    /// This might be different than the mapping size
    pub memory_size: usize,
    pub map_address: usize,
    /// Mapping size of memory in bytes
    pub map_size: usize,
    /// Start padding in bytes
    pub padding_start: usize,
    /// End padding in bytes
    pub padding_end: usize,
}

/// Converts the raw block of memory passed into a program on startup into the process init data
pub fn process_data_from_slice(data: &[u8]) -> Result<(ProcessInitData, &[ProcessMemoryEntry]), PodCastError> {
    let process_init_data = *try_from_bytes(&data[..size_of::<ProcessInitData>()])?;
    let memory_entries = try_cast_slice(&data[size_of::<ProcessInitData>()..])?;

    Ok((process_init_data, memory_entries))
}