use core::{ptr::NonNull, ptr, ops::Deref, mem::size_of};

use rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use thiserror_no_std::Error;
use bit_utils::{Size, PAGE_SIZE, LOWER_HALF_END, KERNEL_RESERVED_START, HIGHER_HALF_START};
use sys::{Memory, MemoryMappingFlags, CapFlags, SysErr, MemoryResizeFlags};

use crate::context::Context;
use crate::prelude::*;
use crate::this_context;

/// This is the first address that is not allowed to be mapped by address space manager
/// 
/// This is the first non canonical address on x86_64
/// The kernel does allow mapping memory in the higher half
/// of the address space, as long as it is below the kernel region,
/// but for now address space manager does not support that
/// 
/// AddrSpaceManager does use the upper half for its internal list of memory capabilities,
/// but nothing else is mapped there
const MAX_MAP_ADDR: usize = LOWER_HALF_END;

#[derive(Debug, Error)]
pub enum AddrSpaceError {
    #[error("Failed to update memory region list: out of memory")]
    RegionListOom,
    #[error("Failed to update meory region list: maximum region list size exceeded")]
    RegionListMaxSizeExceeded,
    #[error("Failed to create ananamous mapping: out of memory")]
    AnanamousMappingOom,
    #[error("Zero size mappings and reservations are not allowed")]
    ZeroSizeMapping,
    #[error("Specified map address causes overlap with another memory region")]
    MappingOverlap,
    #[error("Operation involving padding size, address, or mapping size caused overflow")]
    Overflow,
    #[error("There is no available region in the address space where the mapping will fit")]
    NoAvailableRegion,
    #[error("No mapping at address {0} exists")]
    InvalidAddress(usize),
    #[error("Syscall error when mapping memory: {0:?}")]
    MemorySyscallError(#[from] SysErr),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RegionPadding {
    pub start: Size,
    pub end: Size,
}

#[derive(Debug)]
pub struct MappedRegion {
    pub(crate) memory_cap: Option<Memory>,
    pub(crate) address: usize,
    pub(crate) size: Size,
    pub(crate) padding: RegionPadding,
}

impl MappedRegion {
    fn start_address(&self) -> usize {
        // overflow is already checked at this point
        self.address - self.padding.start.bytes()
    }

    fn end_address(&self) -> usize {
        // overflow is already checked at this point
        self.address + self.size.bytes() + self.padding.end.bytes()
    }
}

impl MappedRegion {
    fn contains_address(&self, address: usize) -> bool {
        if address >= self.address {
            address < (self.address + self.size.bytes_aligned() + self.padding.end.bytes_aligned())
        } else {
            address >= (self.address - self.padding.start.bytes_aligned())
        }
    }
}

/// Maximum possible size of region list in pages
const REGION_LIST_MAX_SIZE: Size = Size::from_pages(4096);

pub trait MappedRegionStorage: Deref<Target = [MappedRegion]> {
    fn len(&self) -> usize;
    
    fn insert(&mut self, index: usize, region: MappedRegion) -> Result<(), AddrSpaceError>;
    fn remove(&mut self, index: usize) -> MappedRegion;
}

/// Stores the mapped regions in a memory capability
pub struct MemoryCapStorage {
    /// Memory capability that stores the region list
    memory: Memory,
    /// Pointer to region list
    data: NonNull<MappedRegion>,
    /// Number of elements in the region lsit
    len: usize,
    /// Total number of elements that the region list has capacity to store
    capacity: usize,
}

impl MemoryCapStorage {
    fn new(aslr_rng: &mut ChaCha20Rng) -> Result<Self, AddrSpaceError> {
        // randomly place region list in higher half memory
        let higher_half_size = KERNEL_RESERVED_START - HIGHER_HALF_START;
        let available_map_positons = 1 + (higher_half_size - REGION_LIST_MAX_SIZE.bytes()) / PAGE_SIZE;

        let map_position = (aslr_rng.next_u64() as usize) % available_map_positons;
        let map_address = HIGHER_HALF_START + map_position * PAGE_SIZE;

        let memory = Memory::new(
            CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
            &this_context().allocator,
            Size::from_pages(1),
        ).or(Err(AddrSpaceError::RegionListOom))?;

        this_context().address_space
            .map_memory(
                &memory,
                map_address,
                None,
                MemoryMappingFlags::READ | MemoryMappingFlags::WRITE,
            ).or(Err(AddrSpaceError::RegionListOom))?;
        
        Ok(MemoryCapStorage {
            capacity: memory.size().bytes() / size_of::<MappedRegion>(),
            memory,
            data: NonNull::new(map_address as *mut MappedRegion).unwrap(),
            len: 0,
        })
    }
}

impl MemoryCapStorage {
    /// Doubles the size of the region list to allow space for more entries
    fn try_grow(&mut self) -> Result<(), AddrSpaceError> {
        // because of max region size, this should not overflow
        let new_size = self.memory.size() * 2;

        if new_size > REGION_LIST_MAX_SIZE {
            return Err(AddrSpaceError::RegionListMaxSizeExceeded);
        }

        this_context().address_space
            .resize_memory(
                &mut self.memory,
                new_size,
                MemoryResizeFlags::IN_PLACE | MemoryResizeFlags::GROW_MAPPING
            ).or(Err(AddrSpaceError::RegionListOom))?;

        self.capacity = new_size.bytes() / size_of::<MappedRegion>();

        Ok(())
    }

    /// Ensures the region list has space for 1 more element
    fn ensure_capacity(&mut self) -> Result<(), AddrSpaceError> {
        if self.len == self.capacity {
            self.try_grow()
        } else {
            Ok(())
        }
    }

    // returns a mutable pointer to the object at the specified index
    unsafe fn off(&mut self, index: usize) -> *mut MappedRegion {
        unsafe { self.data.as_ptr().add(index) }
    }
}

impl Deref for MemoryCapStorage {
    type Target = [MappedRegion];

    fn deref(&self) -> &Self::Target {
        unsafe {
            core::slice::from_raw_parts(self.data.as_ptr(), self.len)
        }
    }
}

impl MappedRegionStorage for MemoryCapStorage {
    fn len(&self) -> usize {
        self.len
    }

    fn insert(&mut self, index: usize, region: MappedRegion) -> Result<(), AddrSpaceError> {
        assert!(index <= self.len);

        self.ensure_capacity()?;

        let ncpy = self.len - index;

        unsafe {
            ptr::copy(self.off(index), self.off(index + 1), ncpy);
            ptr::write(self.off(index), region);
        }

        self.len += 1;

        Ok(())
    }

    fn remove(&mut self, index: usize) -> MappedRegion {
        assert!(index < self.len, "index out of bounds");

        let out = unsafe { ptr::read(self.off(index)) };

        self.len -= 1;
        let ncpy = self.len - index;

        unsafe {
            ptr::copy(self.off(index + 1), self.off(index), ncpy);
        }

        out
    }
}

impl Drop for MemoryCapStorage {
    fn drop(&mut self) {
        for i in 0..self.len {
            unsafe {
                // safety: any values at index less then i are valid for reading
                ptr::drop_in_place(self.off(i));
            }
        }
    }
}

impl MappedRegionStorage for Vec<MappedRegion> {
    fn len(&self) -> usize {
        self.len()
    }

    fn insert(&mut self, index: usize, region: MappedRegion) -> Result<(), AddrSpaceError> {
        Ok(self.insert(index, region))
    }

    fn remove(&mut self, index: usize) -> MappedRegion {
        self.remove(index)
    }
}


pub type LocalAddrSpaceManager = AddrSpaceManager<'static, MemoryCapStorage>;
pub type RemoteAddrSpaceManager<'a> = AddrSpaceManager<'a, Vec<MappedRegion>>;

/// Manages memory that is mapped into address space
pub struct AddrSpaceManager<'a, T: MappedRegionStorage> {
    memory_regions: T,
    /// Used by iter free regions, this is the region at the end
    /// 
    /// This needs to be stored here for lifetimes to work
    end_region: MappedRegion,
    aslr_rng: ChaCha20Rng,
    /// This will be the context where memory is mapped and unmapped
    context: &'a Context,
}

impl LocalAddrSpaceManager {
    /// Creates an AddrSpaceManager for the current process
    pub fn new_local(aslr_seed: [u8; 32]) -> Result<Self, AddrSpaceError> {
        let mut aslr_rng = ChaCha20Rng::from_seed(aslr_seed);

        let mut out = AddrSpaceManager {
            memory_regions: MemoryCapStorage::new(&mut aslr_rng)?,
            end_region: MappedRegion {
                memory_cap: None,
                address: MAX_MAP_ADDR,
                size: Size::default(),
                padding: RegionPadding::default(),
            },
            aslr_rng,
            context: this_context().clone(),
        };

        out.reserve_null_page()?;

        Ok(out)
    }
}

impl<'a> RemoteAddrSpaceManager<'a> {
    /// Creates an AddrSpaceManager for a different process to manage its address space
    pub fn new_remote(aslr_seed: [u8; 32], context: &'a Context) -> Result<Self, AddrSpaceError> {
        let mut out = AddrSpaceManager {
            memory_regions: Vec::new(),
            end_region: MappedRegion {
                memory_cap: None,
                address: MAX_MAP_ADDR,
                size: Size::default(),
                padding: RegionPadding::default(),
            },
            aslr_rng: ChaCha20Rng::from_seed(aslr_seed),
            context,
        };

        out.reserve_null_page()?;

        Ok(out)
    }
}

impl<T: MappedRegionStorage> AddrSpaceManager<'_, T> {
    /*fn get(&self, index: usize) -> Option<&MappedRegion> {
        
    }*/

    /// Inserts the region so it will be in address space order
    /// 
    /// # Returns
    /// 
    /// Returns the index of there the region was inserted
    /// 
    /// # Panics
    /// 
    /// panics if the regions start address is the same as another regions address
    /// 
    /// this does not check for any other type of overlap though, this is assumed to be already checked
    pub(crate) fn insert_region(&mut self, region: MappedRegion) -> Result<usize, AddrSpaceError> {
        let index = self.binary_search_address(region.address).unwrap_err();

        self.memory_regions.insert(index, region)?;

        Ok(index)
    }

    fn remove_region(&mut self, address: usize) -> Result<MappedRegion, AddrSpaceError> {
        let index = self.binary_search_address(address)
            .or(Err(AddrSpaceError::InvalidAddress(address)))?;

        Ok(self.memory_regions.remove(index))
    }

    fn get_region(&self, address: usize) -> Result<&MappedRegion, AddrSpaceError> {
        let index = self.binary_search_address(address)
            .or(Err(AddrSpaceError::InvalidAddress(address)))?;

        Ok(&self.memory_regions[index])
    }

    fn binary_search_address(&self, address: usize) -> Result<usize, usize> {
        self.memory_regions.binary_search_by_key(&address, |region| region.address)
    }

    /// Returns an iteratore over all the free regions
    fn iter_free_regions<'a>(&'a self) -> impl Iterator<Item = (usize, Size)> + 'a {
        let mut prev_addr = 0;
        self.memory_regions.iter()
            .chain(core::iter::once(&self.end_region))
            .map(move |region| {
                let out = (prev_addr, Size::from_bytes(region.start_address() - prev_addr));
                prev_addr = region.end_address();

                out
            })
            .filter(|(_, size)| size.bytes() != 0)
    }

    fn is_region_free(&self, address: usize, size: Size, padding: RegionPadding) -> bool {
        // check for overflow when computing start and end address
        let Some(start_address) = address.checked_sub(padding.start.bytes_aligned()) else {
            return false;
        };

        let Some(end_address) = (try {
            let size_bytes = size.bytes_aligned().checked_mul(PAGE_SIZE)?;
            address.checked_add(size_bytes)?.checked_add(padding.end.bytes_aligned())?
        }) else {
            return false;
        };

        // can't map non canonical or upper half address
        if end_address > MAX_MAP_ADDR {
            return false;
        }

        match self.binary_search_address(start_address) {
            Ok(_) => false,
            Err(index) => {
                (index == 0 || !self.memory_regions[index - 1].contains_address(start_address))
                    && (index == self.memory_regions.len() || !self.memory_regions[index].contains_address(end_address))
            },
        }
    }

    /// Finds a suitable address for the given mapping to fit
    /// 
    /// This uses random number generator to do aslr
    // TODO: align map address to make use of huge page mappings
    fn find_map_address(&mut self, size: Size, padding: RegionPadding) -> Result<usize, AddrSpaceError> {
        let region_size: Option<usize> = try {
            size.bytes_aligned()
                .checked_add(padding.start.bytes_aligned())?
                .checked_add(padding.end.bytes_aligned())?
        };
        let region_size = region_size.ok_or(AddrSpaceError::Overflow)?;

        // do a first pass to compute the total number of possible places the region could be mapped at
        let mut available_map_positions = 0;
        for (_, size) in self.iter_free_regions() {
            if size.bytes() >= region_size {
                available_map_positions += size.pages_rounded() + 1;
            }
        }

        if available_map_positions == 0 {
            return Err(AddrSpaceError::NoAvailableRegion);
        }

        // this will technically lead to a higher chance of memory being mapped
        // lower in the address space, but probably not a big deal
        let mut map_position = (self.aslr_rng.next_u64() as usize) % available_map_positions;

        // do a second pass to find out which address was actually selected
        for (address, size) in self.iter_free_regions() {
            if size.bytes() >= region_size {
                let available_positions = size.pages_rounded() + 1;

                if map_position < available_positions {
                    let base_address = address + map_position * PAGE_SIZE;
                    return Ok(base_address + padding.start.bytes_aligned());
                }

                map_position -= available_positions;
            }
        }

        panic!("could not find map region even though one should have existed");
    }
}

/// Arguments for mapping memory in the address apce manager
#[derive(Debug, Default)]
pub struct MapMemoryArgs {
    /// Memory capability to map, or None for an ananamous mapping
    pub memory: Option<Memory>,
    /// Flags to map memory with
    pub flags: MemoryMappingFlags,
    /// Address to map at, or None to find a suitable address
    pub address: Option<usize>,
    /// Size of memory to map in pages, or None to map the whole thing
    /// 
    /// If `size` and `memory` are None, no memory will be mapped
    /// Padding must also be nonzero, so this will efectively just reserve part of the address space
    /// A padding of 0 and no mapping is not allowed
    /// 
    /// A size of 0 is not allowed
    // TODO: have way to specify at least size mappings, not just exact size mappings
    pub size: Option<Size>,
    /// Padding that will be reserved before and 
    pub padding: RegionPadding,
}

#[derive(Debug, Clone, Copy)]
pub struct MapMemoryResult {
    pub address: usize,
    pub size: Size,
}

impl<T: MappedRegionStorage> AddrSpaceManager<'_, T> {
    /// Maps memory into the address space, see [`MapMemoryArgs`] for more details
    // FIXME: check if padding goes below zero or above max userspace address, or non canonical address
    pub fn map_memory(&mut self, args: MapMemoryArgs) -> Result<MapMemoryResult, AddrSpaceError> {
        let padding = args.padding;

        let (memory, size) = match args.memory {
            Some(memory) => {
                let memory_size = memory.size();
                (Some(memory), memory_size)
            },
            None => {
                if let Some(size) = args.size {
                    let memory = Memory::new(
                        CapFlags::READ | CapFlags::WRITE | CapFlags::PROD,
                        &self.context.allocator,
                        size
                    ).or(Err(AddrSpaceError::AnanamousMappingOom))?;

                    (Some(memory), size)
                } else {
                    (None, Size::default())
                }
            }
        };

        let region_size: Option<usize> = try {
            size.bytes_aligned()
                .checked_add(padding.start.bytes_aligned())?
                .checked_add(padding.end.bytes_aligned())?
        };
        let region_size = region_size.ok_or(AddrSpaceError::Overflow)?;

        if (region_size == 0) || (memory.is_some() && size.is_zero()) {
            return Err(AddrSpaceError::ZeroSizeMapping);
        }

        let address = match args.address {
            Some(address) => {
                if !self.is_region_free(address, size, args.padding) {
                    return Err(AddrSpaceError::MappingOverlap);
                }

                address
            },
            None => self.find_map_address(size, args.padding)?,
        };

        let region = MappedRegion {
            memory_cap: memory,
            address,
            size,
            padding: args.padding,
        };

        let region_index = self.insert_region(region)?;
        let region = self.memory_regions.get(region_index).unwrap();

        if let Some(memory) = &region.memory_cap {
            // TODO: have a way to not specify max size pages
            let result = self.context.address_space
                .map_memory(&memory, address, Some(size), args.flags)
                .map_err(|err| AddrSpaceError::MemorySyscallError(err));

            if let Err(err) = result {
                // panic safety: region was added earlier
                self.remove_region(address).unwrap();

                return Err(err);
            }
        }

        Ok(MapMemoryResult {
            address,
            size,
        })
    }

    /// Gets the memory capability currently in use by the given mapping, or None if none is in use
    pub fn get_mapping_capability(&self, address: usize) -> Result<Option<&Memory>, AddrSpaceError> {
        Ok(self.get_region(address)?.memory_cap.as_ref())
    }

    /// Unmaps the given memory and drops the memory capability
    pub unsafe fn unmap_memory(&mut self, address: usize) -> Result<(), AddrSpaceError> {
        let region = self.remove_region(address)?;

        if region.memory_cap.is_some() {
            self.context.address_space.unmap_memory(address)
                .expect("failed to unmap previously mapped memory");
        }

        Ok(())
    }

    /// Marks the first page (at address 0) as reserved so null dereferences will alwayus cause page fault
    fn reserve_null_page(&mut self) -> Result<(), AddrSpaceError> {
        self.map_memory(MapMemoryArgs {
            memory: None,
            size: None,
            address: Some(0),
            padding: RegionPadding {
                start: Size::default(),
                end: Size::from_pages(1)
            },
            ..Default::default()
        })?;

        Ok(())
    }
}

unsafe impl<T: MappedRegionStorage> Send for AddrSpaceManager<'_, T> {}