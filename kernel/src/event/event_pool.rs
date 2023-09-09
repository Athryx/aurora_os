use core::cmp::{max, min};

use sys::{CapId, CapType};

use crate::alloc::{PaRef, HeapRef};
use crate::cap::memory::MemoryInner;
use crate::prelude::*;
use crate::sched::{ThreadRef, WakeReason};
use crate::sync::IMutex;
use crate::container::{Arc, Weak};
use crate::cap::{CapObject, address_space::AddressSpace, memory::Memory};
use crate::vmem_manager::PageMappingFlags;

use super::UserspaceBuffer;

#[derive(Debug)]
pub struct EventPool {
    inner: IMutex<EventPoolInner>,
}

/// Communicates to calling thread what it needs to do after calling [`await_event`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwaitStatus {
    /// There were events in the event pool and they have now been mapped
    Success,
    /// There were no events in the event pool and the thread must block
    Block,
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
        })
    }

    pub fn await_event(&self) -> KResult<AwaitStatus> {
        let mut inner = self.inner.lock();

        // another thread is already waiting on this event pool
        if inner.waiting_thread.is_some() {
            return Err(SysErr::InvlOp);
        }

        if inner.has_unprocessed_events() {
            inner.swap_buffers()?;

            Ok(AwaitStatus::Success)
        } else {
            // wait for event to arrive
            let thread_ref = ThreadRef::future_ref(&cpu_local_data().current_thread());
            inner.waiting_thread = Some(thread_ref);

            Ok(AwaitStatus::Block)
        }
    }

    pub fn write_event(&self, event_capid: CapId, event_data: &[u8]) -> KResult<()> {
        let mut inner = self.inner.lock();

        // safety: the write buffer is not mapped
        unsafe {
            inner.write_buffer.write_event(event_capid, event_data)?;
        }

        inner.wake_listener();

        Ok(())
    }

    pub fn write_event_from_userspace(&self, event_capid: CapId, event_data: &UserspaceBuffer) -> KResult<()> {
        let mut inner = self.inner.lock();

        // safety: the write buffer is not mapped
        unsafe {
            inner.write_buffer.write_event_from_userspace(event_capid, event_data)?;
        }

        inner.wake_listener();

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

    fn wake_listener(&mut self) {
        if let Some(thread) = self.waiting_thread.take() {
            thread.move_to_ready_list(WakeReason::EventRecieved);
        }
    }

    /// Swaps the buffers so unprocessed events can be processed
    fn swap_buffers(&mut self) -> KResult<()> {
        let (addr_space, map_addr) = self.get_mapping_info()
            .ok_or(SysErr::InvlOp)?;

        let mut addr_space_inner = addr_space.inner();

        // unmap olf mapped buffer
        if let Some(map_size) = self.map_size {
            let memory_inner = self.mapped_buffer.memory.inner_read();

            for (virt_range, _) in memory_inner.iter_mapped_regions(
                map_addr,
                Size::zero(),
                map_size,
            ) {
                // this should not fail because we ensure that memory was already mapped
                addr_space_inner.addr_space.unmap_memory(virt_range)
                    .expect("failed to unmap memory that szhould have been mapped");
            }
        }
        self.mapped_buffer.current_event_offset = 0;

        // map new memory
        let memory_inner = self.write_buffer.memory.inner_read();
        let new_map_size = Size::from_bytes(self.write_buffer.current_event_offset).as_aligned();

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

            unsafe {
                memory.resize_out_of_place(new_size / PAGE_SIZE)?;
            }
        }

        Ok(())
    }

    /// Writes the event into this buffer
    /// 
    /// # Safety
    /// 
    /// This event buffer must not be mapped
    pub unsafe fn write_event(&mut self, event_capid: CapId, event_data: &[u8]) -> KResult<()> {
        let write_size = size_of::<usize>() + event_data.len();
        let mut memory = self.memory.inner_write();

        unsafe {
            self.ensure_capacity(&mut memory, write_size)?;
        }

        unsafe {
            memory.write(self.current_event_offset, &usize::from(event_capid).to_le_bytes());
        }
        self.current_event_offset += size_of::<usize>();

        unsafe {
            memory.write(self.current_event_offset, event_data);
        }
        self.current_event_offset += align_up(event_data.len(), size_of::<usize>());

        Ok(())
    }
    /// Writes the event into this buffer from the userspace buffer
    /// 
    /// # Safety
    /// 
    /// This event buffer must not be mapped
    pub unsafe fn write_event_from_userspace(&mut self, event_capid: CapId, event_data: &UserspaceBuffer) -> KResult<()> {
        let other_memory = event_data.memory.upgrade()
            .ok_or(SysErr::InvlWeak)?;

        let write_size = size_of::<usize>() + event_data.buffer_size;
        let mut memory = self.memory.inner_write();

        unsafe {
            self.ensure_capacity(&mut memory, write_size)?;
        }

        unsafe {
            memory.write(self.current_event_offset, &usize::from(event_capid).to_le_bytes());
        }
        self.current_event_offset += size_of::<usize>();

        unsafe {
            memory.copy_from_memory(
                self.current_event_offset,
                &other_memory.inner_read(),
                event_data.offset..(event_data.offset + event_data.buffer_size),
            );
        }
        self.current_event_offset += align_up(event_data.buffer_size, size_of::<usize>());

        Ok(())
    }
}

#[derive(Debug)]
struct EventPoolMapping {
    address_space: Weak<AddressSpace>,
    mapped_address: VirtAddr,
}