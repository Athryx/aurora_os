use core::cmp::{max, min};

use sys::{CapType, CapId, EventId, MESSAGE_RECIEVED_NUM};

use crate::alloc::{PaRef, HeapRef};
use crate::cap::address_space::{MappingId, AddressSpaceInner, AddrSpaceMapping};
use crate::cap::memory::{MemoryCopySrc, MemoryWriter};
use crate::prelude::*;
use crate::sched::{ThreadRef, WakeReason};
use crate::sync::IMutex;
use crate::container::{Arc, Weak};
use crate::cap::{CapObject, address_space::{AddressSpace, EventPoolMapping as AddrSpaceEventPoolMapping}, memory::{MemoryWriteRegion, WriteResult, Page}};
use crate::vmem_manager::{MapAction, PageMappingOptions};
use crate::cap::channel::{CapabilityTransferInfo, CapabilityWriter};

/// Communicates to calling thread what it needs to do after calling [`await_event`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwaitStatus {
    /// There were events in the event pool and they have now been mapped
    Success {
        event_range: UVirtRange,
    },
    /// There were no events in the event pool and the thread must block
    Block,
}

#[derive(Debug)]
pub struct EventPool {
    inner: IMutex<EventPoolInner>,
    id: MappingId,
    // TODO: remove mapping id
    // it is no longer used for anything in event pool but many addr space methods
    // assume each mapping has a map id so it is easier to keep then to remove
    max_size: Size,
}

impl EventPool {
    pub fn new(page_allocator: PaRef, heap_allocator: HeapRef, max_size: Size) -> KResult<Self> {
        Ok(EventPool {
            inner: IMutex::new(EventPoolInner {
                mapping: None,
                waiting_thread: None,
                mapped_buffer: EventBuffer::new(page_allocator.clone(), heap_allocator.clone(), max_size)?,
                is_buffer_mapped: true,
                write_buffer: EventBuffer::new(page_allocator, heap_allocator, max_size)?,
            }),
            id: MappingId::new(),
            max_size,
        })
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn max_size(&self) -> Size {
        self.max_size
    }

    pub fn await_event(&self) -> KResult<AwaitStatus> {
        let mut inner = self.inner.lock();

        // another thread is already waiting on this event pool
        if inner.waiting_thread.is_some() {
            return Err(SysErr::InvlOp);
        }

        // cannot wait on event pool if it is not mapped
        if inner.mapping.is_none() {
            return Err(SysErr::InvlOp);
        }

        if inner.has_unprocessed_events() {
            let event_range = inner.swap_buffers()?;

            Ok(AwaitStatus::Success { event_range })
        } else {
            // wait for event to arrive
            let thread_ref = ThreadRef::future_ref(&cpu_local_data().current_thread());
            inner.waiting_thread = Some(thread_ref);

            Ok(AwaitStatus::Block)
        }
    }

    /// Writes the event id and event data into this event pool, and potentially wakes a waiting thread
    pub fn write_event<T: MemoryCopySrc + ?Sized>(&self, event_data: &T) -> KResult<Size> {
        let mut inner = self.inner.lock();

        // safety: the write buffer is not mapped
        let write_size = unsafe {
            inner.write_buffer.write_event(event_data)?
        };

        inner.wake_listener()?;

        Ok(write_size)
    }

    /// Writes the event id and event data into this event pool, does not wake listener
    /// 
    /// This version also copies capabilities over, it is used for sending capabilties over channels
    pub fn write_channel_event<T: MemoryCopySrc + ?Sized>(
        &self,
        event_id: EventId,
        reply_cap_id: Option<CapId>,
        event_data: &T,
        cap_transfer_info: CapabilityTransferInfo,
    ) -> KResult<Size> {
        let mut inner = self.inner.lock();

        // safety: the write buffer is not mapped
        unsafe {
            inner.write_buffer.write_channel_event(event_id, reply_cap_id, event_data, cap_transfer_info)
        }
    }

    /// Wakes a thread if it is waiting on the event pool
    pub fn wake_listener(&self) -> KResult<()> {
        self.inner.lock().wake_listener()
    }

    pub fn map_event_pool(this: Arc<Self>, address_space: Arc<AddressSpace>, address: VirtAddr) -> KResult<Size> {
        let mut inner = this.inner.lock();
        let mut addr_space_inner = address_space.inner();

        if inner.mapping.is_some() {
            // event pool is already mapped
            return Err(SysErr::InvlOp);
        }

        let max_size = this.max_size;

        addr_space_inner.mappings.insert_mapping(
            AddrSpaceMapping::EventPool(
                AddrSpaceEventPoolMapping {
                    event_pool: this.clone(),
                    map_range: AVirtRange::new(address, max_size.bytes()),
                }
            ),
        )?;

        inner.mapping = Some(EventPoolMapping {
            address_space: Arc::downgrade(&address_space),
            mapped_address: address,
        });

        Ok(max_size)
    }

    /// Unmaps the event pool from the address space it is currently mapped in memory
    pub fn unmap(&self) -> KResult<()> {
        let mut inner = self.inner.lock();

        let (addr_space, _) = inner.get_mapping_info()
            .ok_or(SysErr::InvlOp)?;

        inner.unmap_mapped_buffer(&mut addr_space.inner())?;

        inner.mapping = None;

        Ok(())
    }
}

impl CapObject for EventPool {
    const TYPE: CapType = CapType::EventPool;
}

#[derive(Debug)]
struct EventPoolInner {
    /// Information about where event pool is mapped
    mapping: Option<EventPoolMapping>,
    waiting_thread: Option<ThreadRef>,
    /// The event buffer currently mapped in userspace
    mapped_buffer: EventBuffer,
    is_buffer_mapped: bool,
    /// The event buffer where new events will be written, currentyl unmapped
    write_buffer: EventBuffer,
}

impl EventPoolInner {
    fn has_unprocessed_events(&self) -> bool {
        self.write_buffer.current_event_offset > 0
    }

    /// If a thread is waiting on this event pool, wakes that thread and swaps buffers
    fn wake_listener(&mut self) -> KResult<()> {
        if let Some(thread) = self.waiting_thread.take() {
            let event_range = self.swap_buffers()?;
            thread.move_to_ready_list(WakeReason::EventPoolEventRecieved { event_range });
        }

        Ok(())
    }

    /// Swaps the buffers so unprocessed events can be processed
    /// 
    /// Returns a virt range representing the new memory range of valid events
    fn swap_buffers(&mut self) -> KResult<UVirtRange> {
        let (addr_space, map_addr) = self.get_mapping_info()
            .ok_or(SysErr::InvlOp)?;

        let mut addr_space_inner = addr_space.inner();

        // unmap old mapped buffer
        self.unmap_mapped_buffer(&mut addr_space_inner)?;

        // map new memory
        let event_size = Size::from_bytes(self.write_buffer.current_event_offset);
        // aligns size up
        let map_page_count = event_size.as_aligned().pages_rounded();

        let mapping_iter = self.write_buffer.pages
            .iter()
            .take(map_page_count)
            .enumerate()
            .map(|(i, page)| {
                MapAction {
                    virt_addr: map_addr + PAGE_SIZE * i,
                    phys_addr: page.phys_addr(),
                    options: PageMappingOptions {
                        read: true,
                        write: true,
                        ..Default::default()
                    },
                }
            });

        // safety: we are only mapping allocated pages that we own
        unsafe {
            addr_space_inner.addr_space.map_many(mapping_iter)?;
        }

        self.is_buffer_mapped = true;

        core::mem::swap(&mut self.mapped_buffer, &mut self.write_buffer);

        Ok(UVirtRange::new(map_addr, event_size.bytes()))
    }

    /// Unmaps the currently mapped buffer if it is mapped
    /// 
    /// Must pass in the locked address space for the current event pool mapping
    fn unmap_mapped_buffer(&mut self, addr_space: &mut AddressSpaceInner) -> KResult<()> {
        let map_addr = self.mapping.as_ref()
            .ok_or(SysErr::InvlOp)?.mapped_address;

        if self.is_buffer_mapped {
            for i in 0..self.mapped_buffer.pages.len() {
                unsafe {
                    addr_space.addr_space.unmap_page(map_addr + PAGE_SIZE * i)
                        .expect("tried to unmap event buffer page which was not mapped");
                }
            }

            self.is_buffer_mapped = false;
        }
        self.mapped_buffer.current_event_offset = 0;

        Ok(())
    }

    fn get_mapping_info(&self) -> Option<(Arc<AddressSpace>, VirtAddr)> {
        let mapping = self.mapping.as_ref()?;
        let address_space = mapping.address_space.upgrade()?;

        Some((address_space, mapping.mapped_address))
    }
}

#[derive(Debug)]
struct EventPoolMapping {
    address_space: Weak<AddressSpace>,
    mapped_address: VirtAddr,
}

/// Region of memory that events can be pushed into
/// 
/// This a stack
#[derive(Debug)]
struct EventBuffer {
    pages: Vec<Page>,
    page_allocator: PaRef,
    /// Offset in memory of the top fo the stack, this is kept 8 byte aligned
    current_event_offset: usize,
    /// Maximum size event buffer is allowed to grow to
    max_size: Size,
}

impl EventBuffer {
    pub fn new(page_allocator: PaRef, heap_allocator: HeapRef, max_size: Size) -> KResult<Self> {
        Ok(EventBuffer {
            pages: Vec::new(heap_allocator),
            page_allocator,
            current_event_offset: 0,
            max_size,
        })
    }

    fn current_capacity(&self) -> Size {
        Size::from_pages(self.pages.len())
    }

    /// Resizes this event buffer to have `page_count` pages of capacity
    /// 
    /// # Safety
    /// 
    /// this event buffet must not be mapped
    unsafe fn resize(&mut self, page_count: usize) -> KResult<()> {
        // reduce page count if it is too big
        while self.pages.len() > page_count {
            self.pages.pop().unwrap();
        }

        // allocate new pages if page count is currently not enough
        while self.pages.len() < page_count {
            let new_page = Page::new(self.page_allocator.clone())?;
            self.pages.push(new_page)?;
        }

        Ok(())
    }

    /// Ensures the event buffer has enough capacity to write `write_size` more bytes in the event buffer
    /// 
    /// # Safety
    /// 
    /// this event buffer must not be mapped
    pub unsafe fn ensure_capacity(&mut self, write_size: usize) -> KResult<()> {
        let required_capacity = align_up(self.current_event_offset + write_size, PAGE_SIZE);
        if required_capacity > self.max_size.bytes() {
            return Err(SysErr::OutOfCapacity);
        }

        let current_capacity = self.current_capacity().bytes();

        if write_size > current_capacity {
            let new_size = max(
                2 * current_capacity,
                required_capacity,
            );
            let new_size = min(new_size, self.max_size.bytes());

            // safety: caller ensures this buffer is not mapped
            unsafe {
                self.resize(new_size / PAGE_SIZE)?;
            }
        }

        Ok(())
    }

    /// Gets a writer for the given size
    /// 
    /// # Safety
    /// 
    /// This event buffer must not be mapped
    unsafe fn get_writer(&mut self, write_size: usize) -> KResult<EventBufferWriter> {
        // safety: caller ensures this buffer is not mapped
        unsafe {
            self.ensure_capacity(write_size)?;
        }

        Ok(EventBufferWriter {
            event_buffer: self,
            current_page_index: self.current_event_offset / PAGE_SIZE,
            current_offset: self.current_event_offset % PAGE_SIZE,
        })
    }

    /// Writes the event into this buffer
    /// 
    /// # Safety
    /// 
    /// This event buffer must not be mapped
    // FIXME: report when memory region is exhausted, and no more data could be written
    pub unsafe fn write_event<T: MemoryCopySrc + ?Sized>(&mut self, event_data: &T) -> KResult<Size> {
        let desired_write_size = align_up(event_data.size(), size_of::<usize>());

        // safety: caller ensures this buffer is not mapped
        let mut writer = unsafe {
            self.get_writer(desired_write_size)?
        };

        let actual_write_size = event_data.copy_to(&mut writer)?;

        self.current_event_offset += align_up(actual_write_size.bytes(), size_of::<usize>());

        Ok(actual_write_size)
    }

    /// Writes a channel event into this buffer and transfers capabilities over
    /// 
    /// # Safety
    /// 
    /// This event buffer must not be mapped
    // FIXME: report when memory region is exhausted, and no more data could be written
    pub unsafe fn write_channel_event<T: MemoryCopySrc + ?Sized>(
        &mut self,
        event_id: EventId,
        reply_cap_id: Option<CapId>,
        event_data: &T,
        cap_transfer_info: CapabilityTransferInfo,
    ) -> KResult<Size> {
        let desired_write_size = 4 * size_of::<usize>() // 1 word for tag, 1 for event id, 1 for reply capid, 1 for data size
            + align_up(event_data.size(), size_of::<usize>());

        // safety: caller ensures this buffer is not mapped
        let mut inner_writer = unsafe {
            self.get_writer(desired_write_size)?
        };

        let mut actual_write_size = Size::zero();

        let mut write_usize = |n: usize| {
            let bytes = n.to_le_bytes();
            actual_write_size += inner_writer.write_region(bytes.as_slice().into())?.write_size;
            Ok(())
        };

        write_usize(MESSAGE_RECIEVED_NUM)?;
        write_usize(event_id.as_u64() as usize)?;

        let cap_id = reply_cap_id.unwrap_or(CapId::null()).into();
        write_usize(cap_id)?;

        let (Some(write_size_ptr), ptr_write_size) = inner_writer.push_usize_ptr()? else {
            // panic safety: get writer ensures the writer is big enough
            panic!("could not write ptr to event pool buffer");
        };
        actual_write_size += ptr_write_size;

        let mut cap_writer = CapabilityWriter::new(cap_transfer_info, inner_writer);
        let event_write_size = event_data.copy_to(&mut cap_writer)?;
        actual_write_size += event_write_size;

        unsafe {
            // safety: inner writer ensures this pointer is valid
            ptr::write(write_size_ptr, event_write_size.bytes());
        }

        self.current_event_offset += align_up(actual_write_size.bytes(), size_of::<usize>());

        Ok(actual_write_size)
    }
}

pub struct EventBufferWriter<'a> {
    event_buffer: &'a EventBuffer,
    current_page_index: usize,
    current_offset: usize,
}

impl MemoryWriter for EventBufferWriter<'_> {
    fn current_ptr(&mut self) -> KResult<*mut u8> {
        let page = &self.event_buffer.pages[self.current_page_index];

        unsafe {
            Ok(page.allocation().as_mut_ptr::<u8>().add(self.current_offset))
        }
    }

    fn write_region(&mut self, region: MemoryWriteRegion) -> KResult<WriteResult> {
        let mut src_offset = 0;

        loop {
            if self.current_page_index == self.event_buffer.pages.len() {
                // no more space left to write events
                return Ok(WriteResult {
                    write_size: Size::from_bytes(src_offset),
                    end_reached: true,
                });
            }

            let write_size = min(PAGE_SIZE - self.current_offset, region.size());

            unsafe {
                let src_ptr = region.ptr().add(src_offset);
                let dst_ptr = self.current_ptr()?;
                core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, write_size);
            }

            // update dst offset
            self.current_offset += write_size;
            if self.current_offset == PAGE_SIZE {
                // finished writing to current page, move to next one
                self.current_offset = 0;
                self.current_page_index += 1;
            }

            src_offset += write_size;
            if src_offset == region.size() {
                // finished writing this region
                return Ok(WriteResult {
                    write_size: Size::from_bytes(region.size()),
                    end_reached: false,
                });
            }
        }
    }
}