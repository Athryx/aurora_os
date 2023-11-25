use core::ops::{RangeBounds, Bound};

use crate::prelude::*;
use crate::alloc::{PaRef, HeapRef};
use crate::sync::{IrwLock, IrwLockReadGuard, IrwLockWriteGuard};
use crate::container::{Weak, Arc, HashMap};
use crate::vmem_manager::{MapAction, VirtAddrSpace, PageMappingOptions};
use super::address_space::{AddressSpace, AddrSpaceMapping, MemoryMapping as AddrSpaceMemoryMapping, AddressSpaceInner};
use super::{CapObject, CapType, address_space::MappingId};

mod memory_writer;
pub use memory_writer::*;
mod page;
pub use page::*;

/// A capability that represents memory that can be mapped into a process
#[derive(Debug)]
pub struct Memory {
    id: MappingId,
    inner: IrwLock<MemoryInner>,
}

impl Memory {
    /// Returns an error is pages is size 0
    pub fn new_with_page_source(
        mut page_allocator: PaRef,
        heap_allocator: HeapRef,
        page_count: usize,
        page_source: PageSource,
    ) -> KResult<Self> {
        if page_count == 0 {
            return Err(SysErr::InvlArgs);
        }

        let size = Size::try_from_pages(page_count).ok_or(SysErr::Overflow)?;

        let mut pages = Vec::try_with_capacity(heap_allocator.clone(), page_count)?;

        pages.extend(page_source.create_pages(page_count, &mut page_allocator)?)?;

        let inner = MemoryInner {
            pages,
            size,
            page_allocator,
            mappings: HashMap::new(heap_allocator),
        };

        Ok(Memory {
            id: MappingId::new(),
            inner: IrwLock::new(inner),
        })
    }

    /// Maps this memory capability into the given addr_spce at the given location
    /// 
    /// # Returns
    /// 
    /// returns the size of the mapping
    /// 
    /// # Locking
    /// 
    /// acquires the memory inner lock for write
    /// then acquires the addr_space inner lock
    pub fn map_memory(this: Arc<Self>, addr_space: Arc<AddressSpace>, args: MapMemoryArgs) -> KResult<Size> {
        let mut inner = this.inner_write();
        let mut addr_space_inner = addr_space.inner();

        let location = inner.map_memory_args_to_location(args)
            .ok_or(SysErr::InvlArgs)?;

        // do this first to make sure mapping is valid region
        let _ = inner.mapping_iter(location)
            .ok_or(SysErr::InvlMemZone)?;

        let mapping_id = MappingId::new();
        let mapping = AddrSpaceMemoryMapping {
            memory: this.clone(),
            location,
            mapping_id,
        };

        addr_space_inner.mappings.insert_mapping(AddrSpaceMapping::Memory(mapping))?;

        let mapping = MemoryMapping {
            addr_space: Arc::downgrade(&addr_space),
            location,
        };
        let result = inner.mappings.insert(mapping_id, mapping);

        if let Err(error) = result {
            // panic safety: this mapping was just inserted
            addr_space_inner.mappings.remove_mapping_from_id(mapping_id).unwrap();
            return Err(error);
        }

        let mapping_iter = inner.mapping_iter(location)
            .ok_or(SysErr::InvlMemZone)?;

        // safety: mapping_iter ensures the regions are valid to map
        let result = unsafe {
            addr_space_inner.addr_space.map_many(mapping_iter)
        };

        if let Err(error) = result {
            // panic safety: these mappings were just inserted
            addr_space_inner.mappings.remove_mapping_from_id(mapping_id).unwrap();
            inner.mappings.remove(&mapping_id).unwrap();
            return Err(error);
        }

        Ok(location.map_size)
    }

    /// Maps this memory capability from the given addr_spce at the given address
    /// 
    /// # Locking
    /// 
    /// acquires the memory inner lock for write
    /// then acquires the addr_space inner lock
    pub fn unmap_memory(&self, address_space: &AddressSpace, address: VirtAddr) -> KResult<()> {
        let mut inner = self.inner_write();
        let mut addr_space_inner = address_space.inner();

        inner.unmap_memory_inner(&mut addr_space_inner, address)
    }

    pub fn update_mapping(&self, address_space: &AddressSpace, address: VirtAddr, args: UpdateMappingAgs) -> KResult<Size> {
        let mut inner = self.inner_write();
        let mut addr_space_inner = address_space.inner();

        inner.update_mapping_inner(&mut addr_space_inner, address, args)
    }

    pub fn resize(&self, new_size: Size, page_source: PageSource) -> KResult<Size> {
        let mut inner = self.inner_write();

        if inner.mappings.len() != 0 {
            // cannot resize memory if it is mapped
            return Err(SysErr::InvlOp);
        }

        // safety: this memory is not maped anywhere
        unsafe {
            inner.resize_with_page_source(new_size.pages_rounded(), page_source)?;
        }

        Ok(inner.size)
    }

    /// Resizes the memory capability to the new size
    /// 
    /// If the memory is mapped in 1 place, and the memory size is increased, and extend mapping is true,
    /// the 1 mapping will have its size extended to the end of the memory capability
    /// It is possible for the mapping extending operation to fail, and resize_in_place to report failure,
    /// but the memory may still have increased in size
    /// 
    /// # Returns
    /// 
    /// The new size of the memory
    pub fn resize_in_place(&self, new_size: Size, extend_mapping: bool, page_source: PageSource) -> KResult<Size> {
        if new_size.pages_rounded() == 0 {
            return Err(SysErr::InvlArgs);
        }

        let mut inner = self.inner_write();

        if inner.size == new_size {
            return Ok(inner.size)
        }

        if inner.mappings.len() == 0 {
            // safety: this memory is not maped anywhere
            unsafe {
                inner.resize_with_page_source(new_size.pages_rounded(), page_source)?;
                Ok(inner.size)
            }
        } else if inner.mappings.len() == 1 {
            // panic safety: this iterator will yield 1 element
            let (_, mapping) = inner.mappings.iter().next().unwrap();
            let map_addr = mapping.location.map_addr;

            let Some(addr_space) = mapping.addr_space.upgrade() else {
                // safety: this memory is not maped anywhere if address space if dropped
                unsafe {
                    inner.resize_with_page_source(new_size.pages_rounded(), page_source)?;
                    return Ok(inner.size)
                }
            };

            let mut addr_space_inner = addr_space.inner();

            if new_size > inner.size {
                // grow memory
                unsafe {
                    inner.resize_with_page_source(new_size.pages_rounded(), page_source)?;
                }

                if extend_mapping {
                    inner.update_mapping_inner(&mut addr_space_inner, map_addr, UpdateMappingAgs {
                        size: UpdateValue::Change(None),
                        ..Default::default()
                    })?;
                }
            } else if new_size < inner.size {
                // shrink memory
                // the end page index of the mapping
                let mapping_end_index = mapping.location.offset.pages_rounded() + mapping.location.map_size.pages_rounded();
                
                if mapping_end_index <= new_size.pages_rounded() {
                    // we do not shrink smaller than the mapping, it is ok to shrink without updating mapping
                    unsafe {
                        inner.resize_with_page_source(new_size.pages_rounded(), page_source)?;
                    }
                } else {
                    let mapping_decrease_amount = mapping_end_index - new_size.pages_rounded();

                    if mapping_decrease_amount >= mapping.location.map_size.pages_rounded() {
                        // the mapping needs to be unmapped, it is entirely past the new end of the memory capability
                        inner.unmap_memory_inner(&mut addr_space_inner, map_addr)?;
                    } else {
                        let new_mapping_size = mapping.location.map_size.pages_rounded() - mapping_decrease_amount;
                        inner.update_mapping_inner(&mut addr_space_inner, map_addr, UpdateMappingAgs {
                            size: UpdateValue::Change(Some(Size::from_pages(new_mapping_size))),
                            ..Default::default()
                        })?;
                    }

                    // safety: it is now safe to shrink pages because mappings have been shrunk
                    unsafe {
                        inner.resize_with_page_source(new_size.pages_rounded(), page_source)?;
                    }
                }
            }

            Ok(inner.size)
        } else {
            // cannot resize if memory is mapped in more than 1 place
            Err(SysErr::InvlOp)
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn inner_read(&self) -> IrwLockReadGuard<MemoryInner> {
        self.inner.read()
    }

    pub fn inner_write(&self) -> IrwLockWriteGuard<MemoryInner> {
        self.inner.write()
    }
}

impl CapObject for Memory {
    const TYPE: CapType = CapType::Memory;
}

/// A location where a memory capability is mapped in an address space
#[derive(Debug, Clone, Copy)]
pub struct MemoryMappingLocation {
    pub map_addr: VirtAddr,
    pub map_size: Size,
    /// Offset into memory capability where mapping is from
    pub offset: Size,
    pub options: PageMappingOptions,
}

impl MemoryMappingLocation {
    pub fn map_range(&self) -> AVirtRange {
        AVirtRange::new(self.map_addr, self.offset.bytes())
    }
}

#[derive(Debug)]
struct MemoryMapping {
    addr_space: Weak<AddressSpace>,
    location: MemoryMappingLocation,
}

#[derive(Debug, Clone, Copy)]
pub struct MapMemoryArgs {
    pub map_addr: VirtAddr,
    pub map_size: Option<Size>,
    pub offset: Size,
    pub options: PageMappingOptions,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum UpdateValue<T> {
    Change(T),
    #[default]
    KeepSame,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UpdateMappingAgs {
    pub size: UpdateValue<Option<Size>>,
    pub options: UpdateValue<PageMappingOptions>,
}

#[derive(Debug)]
pub struct MemoryInner {
    pages: Vec<PageData>,
    /// Total size of all allocations
    size: Size,
    page_allocator: PaRef,
    /// All places where this memory capability is currently mapped
    mappings: HashMap<MappingId, MemoryMapping>,
}

impl MemoryInner {
    /// Returns the total size of this memory
    pub fn size(&self) -> Size {
        self.size
    }

    pub fn get_map_size(&self, map_size: Option<Size>, offset: Size) -> Option<Size> {
        if offset >= self.size {
            return None;
        }

        if let Some(size) = map_size {
            if offset + size > self.size {
                // mapping is too big
                None
            } else {
                Some(size)
            }
        } else {
            Some(self.size - offset)
        }
    }

    /// Converts the map memory args to a location which they would map
    pub fn map_memory_args_to_location(&self, args: MapMemoryArgs) -> Option<MemoryMappingLocation> {
        let map_size = self.get_map_size(args.map_size, args.offset)?;

        Some(MemoryMappingLocation {
            map_addr: args.map_addr,
            map_size,
            offset: args.offset,
            options: args.options,
        })
    }

    pub fn update_mapping_inner(
        &mut self,
        addr_space: &mut AddressSpaceInner,
        address: VirtAddr,
        args: UpdateMappingAgs,
    ) -> KResult<Size> {
        let AddressSpaceInner {
            addr_space,
            mappings,
        } = addr_space;

        let mapping = mappings.get_mapping_from_address_mut(address)
            .ok_or(SysErr::InvlVirtAddr)?;

        let AddrSpaceMapping::Memory(mapping) = mapping else {
            // mapping is event pool, we can't update it
            return Err(SysErr::InvlOp);
        };

        if let UpdateValue::Change(new_size) = args.size {
            let mut new_location = mapping.location;

            let old_size = mapping.location.map_size;
            let new_size = self.get_map_size(new_size, mapping.location.offset)
                .ok_or(SysErr::InvlArgs)?;

            if new_size > old_size {
                new_location.map_size = new_size;

                let mut map_location = new_location;
                map_location.map_addr += old_size.bytes();
                map_location.offset += old_size;
                map_location.map_size -= old_size;

                // panic safety: new size is already checked to be inbounds
                let mapping_iter = self.mapping_iter(map_location).unwrap();
                unsafe {
                    addr_space.map_many(mapping_iter)?;
                }
            } else if new_size < old_size {
                let mut unmap_location = new_location;
                unmap_location.map_addr += new_size.bytes();
                unmap_location.offset += new_size;
                unmap_location.map_size -= new_size;

                new_location.map_size = new_size;

                self.unmap_location(addr_space, unmap_location);
            }

            mapping.location = new_location;
            self.mappings.get_mut(&mapping.mapping_id).unwrap().location = new_location;
        }

        if let UpdateValue::Change(options) = args.options {
            let mut new_location = mapping.location;
            new_location.options = options;

            let mapping_iter = self.mapping_iter(new_location).unwrap();
            unsafe {
                addr_space.map_many(mapping_iter)?;
            }

            mapping.location = new_location;
            self.mappings.get_mut(&mapping.mapping_id).unwrap().location = new_location;
        }

        Ok(mapping.location.map_size)
    }

    pub fn unmap_memory_inner(&mut self, addr_space: &mut AddressSpaceInner, address: VirtAddr) -> KResult<()> {
        let mapping = addr_space.mappings.get_mapping_from_address(address)
            .ok_or(SysErr::InvlVirtAddr)?;

        if !matches!(mapping, AddrSpaceMapping::Memory(_)) {
            // mapping is event pool, we can't remove it
            return Err(SysErr::InvlOp);
        }

        let mapping = addr_space.mappings.remove_mapping_from_address(address)
            .ok_or(SysErr::InvlVirtAddr)?;

        let mapping = self.mappings.remove(&mapping.map_id())
            .expect("no mapping present in memory capability");

        // panic safety: if this region was mapped, the pages should exist
        self.unmap_location(&mut addr_space.addr_space, mapping.location);

        Ok(())
    }

    /// Unmaps the memory at the given location
    /// 
    /// Panics if the memory was not mapped therre
    pub fn unmap_location(&self, addr_space: &mut VirtAddrSpace, location: MemoryMappingLocation) {
        // panic safety: if this region was mapped, the pages should exist
        for (i, page) in self.get_pages_for_location(location).unwrap().iter().enumerate() {
            if let PageData::Owned(_) | PageData::Cow(_) = page {
                let virt_addr = location.map_addr + PAGE_SIZE * i;
                unsafe {
                    addr_space.unmap_page(virt_addr).expect("failed to unmap page");
                }
            }
        }
    }

    /// Resizes the memory to hav `new_page_count` pages
    /// 
    /// New pages will be filled with the page source
    /// 
    /// # Safety
    /// 
    /// Currently mapped pages must not be unmapped
    unsafe fn resize_with_page_source(
        &mut self,
        new_page_count: usize,
        page_source: PageSource,
    ) -> KResult<()> {
        if new_page_count == 0 {
            return Err(SysErr::InvlArgs);
        }

        let new_size = Size::try_from_pages(new_page_count).ok_or(SysErr::Overflow)?;

        if new_size > self.size {
            let increase_amount = new_page_count - self.pages.len();
            self.pages.extend(page_source.create_pages(increase_amount, &mut self.page_allocator)?)?;
        } else if new_size < self.size {
            self.pages.truncate(new_page_count);
        }

        self.size = new_size;

        Ok(())
    }

    pub fn copy_from<T: MemoryCopySrc + ?Sized>(&mut self, range: impl RangeBounds<usize>, src: &T) -> KResult<Size> {
        let mut writer = self.create_memory_writer(range).ok_or(SysErr::InvlMemZone)?;

        src.copy_to(&mut writer)
    }

    pub fn create_memory_writer(&mut self, range: impl RangeBounds<usize>) -> Option<PlainMemoryWriter> {
        // start byte inclusive
        let start = match range.start_bound() {
            Bound::Included(n) => *n,
            Bound::Excluded(n) => n + 1,
            Bound::Unbounded => 0,
        };

        // end byte exclusive
        let end = match range.end_bound() {
            Bound::Included(n) => n + 1,
            Bound::Excluded(n) => *n,
            Bound::Unbounded => self.size.bytes(),
        };

        if end > self.size.bytes() || start >= end {
            None
        } else {
            Some(PlainMemoryWriter {
                memory: self,
                page_index: start / PAGE_SIZE,
                offset: start,
                end_offset: end,
            })
        }
    }

    /// Looks at all places where the given page is mapped in memory, and modifies the mapping to be a new location
    /// 
    /// Call this after updating page array for existing page
    pub unsafe fn remap_all_mappings_for_page_index(&self, page_index: usize) -> KResult<()> {
        for (_, mapping) in self.mappings.iter() {
            let start_page_index = mapping.location.offset.pages().unwrap();
            if start_page_index > page_index {
                continue;
            }

            let mapping_page_size = mapping.location.map_size.pages_rounded();
            let end_page_index = start_page_index + mapping_page_size;
            if page_index >= end_page_index {
                continue;
            }

            let mapping_virt_offset = (page_index - start_page_index) * PAGE_SIZE;
            let map_addr = mapping.location.map_addr + mapping_virt_offset;

            let Some(address_space) = mapping.addr_space.upgrade() else {
                continue;
            };

            let mut addr_space_inner = address_space.inner();

            // FIXME: don't panic if these maps fail
            // currently no good way to recover from these failing
            match &self.pages[page_index] {
                PageData::Owned(page) => unsafe {
                    addr_space_inner.addr_space.map_page(
                        map_addr,
                        page.phys_addr(),
                        mapping.location.options,
                    ).unwrap();
                },
                PageData::Cow(page) => unsafe {
                    addr_space_inner.addr_space.map_page(
                        map_addr,
                        page.phys_addr(),
                        // remove write flag for copy on write pages
                        mapping.location.options.writable(false),
                    ).unwrap();
                },
                PageData::LazyAlloc | PageData::LazyZeroAlloc => unsafe {
                    addr_space_inner.addr_space.unmap_page(map_addr);
                },
            }
        }
        
        Ok(())
    }

    pub unsafe fn set_page(&mut self, page_index: usize, page: PageData) -> KResult<()> {
        let old_page = core::mem::replace(&mut self.pages[page_index], page);
    
        let result = unsafe {
            self.remap_all_mappings_for_page_index(page_index)
        };

        if result.is_err() {
            self.pages[page_index] = old_page;
        }

        result
    }

    fn get_page_assuming_owned(&self, page_index: usize) -> &Page {
        match &self.pages[page_index] {
            PageData::Owned(page) => page,
            _ => panic!("expected owned page")
        }
    }

    fn get_page_assuming_owned_mut(&mut self, page_index: usize) -> &mut Page {
        match &mut self.pages[page_index] {
            PageData::Owned(page) => page,
            _ => panic!("expected owned page")
        }
    }

    /// Gets the page which can be written to
    /// 
    /// This will allocate lazily allocted pages and resolve copy on write pages and remap them
    /// 
    /// # Panics
    /// 
    /// Panics if `page_index` is out of bounds in the page vec
    pub fn get_page_for_writing(&mut self, page_index: usize) -> KResult<&mut Page> {
        match &self.pages[page_index] {
            PageData::Owned(_) => (),
            PageData::Cow(_) => {
                // temporarilly replace with lazy alloc
                // we will replace it later while still holding lock so it should never cause a lazy alloc
                let data = core::mem::replace(&mut self.pages[page_index], PageData::LazyAlloc);
                let PageData::Cow(data) = data else {
                    unreachable!();
                };

                let new_page = if Arc::strong_count(&data) == 1 {
                    Arc::into_inner(data).unwrap()
                } else {
                    data.create_copy(self.page_allocator.clone())?
                };

                unsafe {
                    self.set_page(page_index, PageData::Owned(new_page))?;
                }
            },
            PageData::LazyAlloc => {
                let new_page = Page::new(self.page_allocator.clone())?;
                unsafe {
                    self.set_page(page_index, PageData::Owned(new_page))?;
                }
            },
            PageData::LazyZeroAlloc => {
                let new_page = Page::new_zeroed(self.page_allocator.clone())?;
                unsafe {
                    self.set_page(page_index, PageData::Owned(new_page))?;
                }
            },
        }

        Ok(self.get_page_assuming_owned_mut(page_index))
    }

    pub fn get_page_for_reading(&mut self, page_index: usize) -> KResult<&Page> {
        match &self.pages[page_index] {
            PageData::Owned(_) => Ok(self.get_page_assuming_owned(page_index)),
            PageData::Cow(_) => {
                // ugly hack to get around borrow checker limitation
                let PageData::Cow(page) = &self.pages[page_index] else {
                    unreachable!();
                };
                Ok(page)
            },
            // TODO: we don't actually need to allocatore for lazy alloc page
            // just have a global page which is zeroed
            PageData::LazyAlloc => {
                let new_page = Page::new(self.page_allocator.clone())?;
                unsafe {
                    self.set_page(page_index, PageData::Owned(new_page))?;
                }
                Ok(self.get_page_assuming_owned(page_index))
            },
            PageData::LazyZeroAlloc => {
                let new_page = Page::new_zeroed(self.page_allocator.clone())?;
                unsafe {
                    self.set_page(page_index, PageData::Owned(new_page))?;
                }
                Ok(self.get_page_assuming_owned(page_index))
            },
        }
    }

    /// Zeros this entire memory capability
    /// 
    /// # Safety
    /// 
    /// Must not write to any memory used by anything else, or a place that userspace doesn't expect
    /// 
    /// # Notes
    /// 
    /// This may fail partway through, and some pages will be zeroed while others won't be
    pub unsafe fn zero(&mut self) -> KResult<()> {
        for i in 0..self.pages.len() {
            let page = self.get_page_for_writing(i)?;
            unsafe {
                page.zero();
            }
        }

        Ok(())
    }

    /// Gets the pages that correspond to the given mapping location
    fn get_pages_for_location(&self, location: MemoryMappingLocation) -> Option<&[PageData]> {
        let map_start_page_index = location.offset.pages_rounded();
        let map_page_len = location.map_size.pages_rounded();

        self.pages.get(map_start_page_index..(map_start_page_index + map_page_len))
    }
}

#[derive(Debug, Clone)]
struct MemoryMappingIter<'a> {
    pages: &'a [PageData],
    index: usize,
    base_addr: VirtAddr,
    options: PageMappingOptions,
}

impl<'a> Iterator for MemoryMappingIter<'a> {
    type Item = MapAction;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.index == self.pages.len() {
                return None;
            } else {
                let page = &self.pages[self.index];
                let virt_addr = self.base_addr + PAGE_SIZE * self.index;

                self.index += 1;

                match page {
                    PageData::Owned(page) => return Some(MapAction {
                        virt_addr,
                        phys_addr: page.phys_addr(),
                        options: self.options,
                    }),
                    PageData::Cow(page) => return Some(MapAction {
                        virt_addr,
                        phys_addr: page.phys_addr(),
                        options: self.options.writable(false),
                    }),
                    PageData::LazyAlloc | PageData::LazyZeroAlloc => continue,
                }
            }
        }
    }
}

impl MemoryInner {
    /// Returns an iterator over the mapping actions needed to map the given location
    fn mapping_iter(&self, map_location: MemoryMappingLocation) -> Option<MemoryMappingIter> {
        let pages_to_map = self.get_pages_for_location(map_location)?;

        Some(MemoryMappingIter {
            pages: pages_to_map,
            index: 0,
            base_addr: map_location.map_addr,
            options: map_location.options,
        })
    }
}