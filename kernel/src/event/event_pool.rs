use core::cmp::{max, min};

use sys::{CapType, CapId, EventId, MESSAGE_RECIEVED_NUM};

use crate::alloc::{PaRef, HeapRef};
use crate::cap::address_space::{MappingId, AddressSpaceInner};
use crate::cap::memory::{MemoryInner, MemoryCopySrc, PlainMemoryWriter, MemoryWriter};
use crate::prelude::*;
use crate::sched::{ThreadRef, WakeReason};
use crate::sync::IMutex;
use crate::container::{Arc, Weak};
use crate::cap::{CapObject, address_space::AddressSpace, memory::Memory};
use crate::vmem_manager::PageMappingFlags;
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
    max_size: Size,
}

impl EventPool {
    pub fn new(page_allocator: PaRef, heap_allocator: HeapRef, max_size: Size) -> KResult<Self> {
        Ok(EventPool {
            inner: IMutex::new(EventPoolInner {
                mapping: None,
                waiting_thread: None,
                mapped_buffer: EventBuffer::new(page_allocator.clone(), heap_allocator.clone(), max_size)?,
                map_size: None,
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

    /// Tells the event pool where in memory it is mapped
    /// 
    /// Souldn't be used directly, use [`AddressSpace::map_event_pool`] instead
    pub fn set_mapping_data(&self, address_space: Weak<AddressSpace>, address: VirtAddr) -> KResult<()> {
        let mut inner = self.inner.lock();

        if inner.mapping.is_some() {
            // event pool is already mapped
            Err(SysErr::InvlOp)
        } else {
            inner.mapping = Some(EventPoolMapping {
                address_space,
                mapped_address: address,
            });

            Ok(())
        }
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
    /// Size of the currently mapped buffer, or None if nothing is mapped (happens before await_event is called once)
    map_size: Option<Size>,
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
        let memory_inner = self.write_buffer.memory.inner_read();
        let event_size = Size::from_bytes(self.write_buffer.current_event_offset);
        let new_map_size = event_size.as_aligned();

        addr_space_inner.addr_space.map_many(
            memory_inner.iter_mapped_regions(
                map_addr,
                Size::zero(),
                new_map_size,
            ),
            PageMappingFlags::READ | PageMappingFlags::WRITE | PageMappingFlags::USER,
        )?;
        drop(memory_inner);

        self.map_size = Some(new_map_size);

        core::mem::swap(&mut self.mapped_buffer, &mut self.write_buffer);

        Ok(UVirtRange::new(map_addr, event_size.bytes()))
    }

    /// Unmaps the currently mapped buffer if it is mapped
    /// 
    /// Must pass in the locked address space for the current event pool mapping
    fn unmap_mapped_buffer(&mut self, addr_space: &mut AddressSpaceInner) -> KResult<()> {
        let map_addr = self.mapping.as_ref()
            .ok_or(SysErr::InvlOp)?.mapped_address;

        // map size will be some only if buffer is currently mapped
        if let Some(map_size) = self.map_size {
            let memory_inner = self.mapped_buffer.memory.inner_read();

            for (virt_range, _) in memory_inner.iter_mapped_regions(
                map_addr,
                Size::zero(),
                map_size,
            ) {
                // this should not fail because we ensure that memory was already mapped
                addr_space.addr_space.unmap_memory(virt_range)
                    .expect("failed to unmap memory that szhould have been mapped");
            }
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

/// Region of memory that events can be pushed into
/// 
/// This a stack
#[derive(Debug)]
struct EventBuffer {
    memory: Memory,
    /// Offset in memory of the top fo the stack, this is kept 8 byte aligned
    current_event_offset: usize,
    /// Maximum size event buffer is allowed to grow to
    max_size: Size,
}

impl EventBuffer {
    pub fn new(page_allocator: PaRef, heap_allocator: HeapRef, max_size: Size) -> KResult<Self> {
        Ok(EventBuffer {
            // TODO: don't have event buffer start out at 1 page size
            memory: Memory::new(page_allocator, heap_allocator, 1)?,
            current_event_offset: 0,
            max_size,
        })
    }

    /// Ensures the event buffer has enough capacity to write `write_size` more bytes in the event buffer
    /// 
    /// # Safety
    /// 
    /// `memory` must not be mapped
    pub unsafe fn ensure_capacity(&self, memory: &mut MemoryInner, write_size: usize) -> KResult<()> {
        let required_capacity = align_up(self.current_event_offset + write_size, PAGE_SIZE);
        if required_capacity > self.max_size.bytes() {
            return Err(SysErr::OutOfCapacity);
        }

        let current_capacity = memory.size().bytes();

        if write_size > current_capacity {
            let new_size = max(
                2 * current_capacity,
                required_capacity,
            );
            let new_size = min(new_size, self.max_size.bytes());

            // safety: caller ensures this buffer is not mapped
            unsafe {
                memory.resize_out_of_place(new_size / PAGE_SIZE)?;
            }
        }

        Ok(())
    }

    /// Gets a writer for the given size
    /// 
    /// # Safety
    /// 
    /// This event buffer must not be mapped
    unsafe fn get_writer<'a>(&self, memory: &'a mut MemoryInner, write_size: usize) -> KResult<PlainMemoryWriter<'a>> {
        // safety: caller ensures this buffer is not mapped
        unsafe {
            self.ensure_capacity(memory, write_size)?;
        }

        // panic safety: ensure capacity allocates enough capacity so this shouldn't fail
        Ok(memory.create_memory_writer(self.current_event_offset..).unwrap())
    }

    /// Writes the event into this buffer
    /// 
    /// # Safety
    /// 
    /// This event buffer must not be mapped
    // FIXME: report when memory region is exhausted, and no more data could be written
    pub unsafe fn write_event<T: MemoryCopySrc + ?Sized>(&mut self, event_data: &T) -> KResult<Size> {
        let desired_write_size = align_up(event_data.size(), size_of::<usize>());
        let mut memory = self.memory.inner_write();

        // safety: caller ensures this buffer is not mapped
        let mut writer = unsafe {
            self.get_writer(&mut memory, desired_write_size)?
        };

        let actual_write_size = event_data.copy_to(&mut writer)?;

        self.current_event_offset += align_up(actual_write_size.bytes(), size_of::<usize>());

        Ok(actual_write_size)
    }

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

        let mut memory = self.memory.inner_write();

        // safety: caller ensures this buffer is not mapped
        let mut inner_writer = unsafe {
            self.get_writer(&mut memory, desired_write_size)?
        };

        let mut actual_write_size = Size::zero();

        let mut write_usize = |n: usize| {
            let bytes = n.to_le_bytes();
            actual_write_size += inner_writer.write_region(bytes.as_slice().into()).write_size;
        };

        write_usize(MESSAGE_RECIEVED_NUM);
        write_usize(event_id.as_u64() as usize);
        
        let cap_id = reply_cap_id.unwrap_or(CapId::null()).into();
        write_usize(cap_id);

        // panic safety: get writer ensures the writer is big enough
        let write_size_ptr = inner_writer.push_usize_ptr().unwrap();

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

#[derive(Debug)]
struct EventPoolMapping {
    address_space: Weak<AddressSpace>,
    mapped_address: VirtAddr,
}