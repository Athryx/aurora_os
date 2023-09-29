use sys::{CapId, CapFlags};

use crate::prelude::*;
use crate::cap::memory::{MemoryWriter, PlainMemoryWriter, WriteResult, MemoryWriteRegion};
use crate::cap::capability_space::{CapabilitySpace, CapCloneWeakness};

#[derive(Clone, Copy)]
pub struct CapabilityTransferInfo<'a> {
    pub src_cspace: &'a CapabilitySpace,
    pub dst_cspace: &'a CapabilitySpace,
}

/// A MemoryWriter which also transfers capabilities
/// 
/// This is used to transfer capabilities when they are sent over a channel
pub struct CapabilityWriter<'a> {
    cap_transfer_info: CapabilityTransferInfo<'a>,
    copy_count: Option<CapabilityCopyCount>,
    inner_writer: PlainMemoryWriter<'a>,
}

impl<'a> CapabilityWriter<'a> {
    pub fn new(cap_transfer_info: CapabilityTransferInfo<'a>, output_writer: PlainMemoryWriter<'a>) -> Self {
        CapabilityWriter {
            cap_transfer_info,
            copy_count: None,
            inner_writer: output_writer,
        }
    }
}

impl MemoryWriter for CapabilityWriter<'_> {
    fn write_region(&mut self, mut region: MemoryWriteRegion) -> WriteResult {
        if self.copy_count.is_none() {
            // initialize copy count if it i not initialized

            let Some(cap_count) = region.read_value::<usize>() else {
                return WriteResult {
                    write_size: Size::zero(),
                    end_reached: true,
                };
            };

            let dst_count_ptr = self.inner_writer.current_offset_ptr() as *mut usize;

            // write 1 usize of 0
            let zero = usize::to_le_bytes(0);
            let write_result = self.inner_writer.write_region(zero.as_slice().into());

            // if there is not enough space for the dest count ptr, return early
            if write_result.write_size.bytes() < size_of::<usize>() {
                return write_result;
            }

            self.copy_count = Some(CapabilityCopyCount {
                remaining_cap_count: cap_count,
                dst_count_ptr,
                cap_id_buffer: [0; size_of::<usize>()],
                cap_id_current_read_count: 0,
            });
        }

        let mut write_size = Size::zero();

        // panic safety: this is ensured to be initialized at this point
        let copy_count = self.copy_count.as_mut().unwrap();

        while let Some(cap_id) = copy_count.get_capid_from_write_region(&mut region) {
            let new_cap_id: KResult<CapId> = try {
                let cap_id = CapId::try_from(cap_id)
                    .ok_or(SysErr::InvlId)?;

                CapabilitySpace::cap_clone(
                    self.cap_transfer_info.dst_cspace,
                    self.cap_transfer_info.src_cspace,
                    cap_id,
                    CapFlags::all(),
                    CapCloneWeakness::KeepSame,
                    false,
                    false,
                )?
            };

            let new_cap_id = new_cap_id.unwrap_or(CapId::null());
            let new_cap_id_bytes = usize::from(new_cap_id).to_le_bytes();

            let write_result = self.inner_writer.write_region(new_cap_id_bytes.as_slice().into());

            write_size += write_result.write_size;
        }

        let mut inner_write_result = self.inner_writer.write_region(region);
        inner_write_result.write_size += write_size;

        inner_write_result
    }
}

struct CapabilityCopyCount {
    /// The number of remaining capabilities to be copied
    remaining_cap_count: usize,
    /// The pointer to the destination counter
    /// 
    /// Incramented everytime 1 capability is copied
    dst_count_ptr: *mut usize,
    /// this buffer saves bytes read from a previous region if only a section of the id was read
    cap_id_buffer: [u8; size_of::<usize>()],
    /// How many valid bytes are currently in the cap_id_buffer
    cap_id_current_read_count: usize,
}

impl CapabilityCopyCount {
    fn inc_copy_count(&mut self) {
        // safety: this address is ensured to be valid when constructing a capability writer
        unsafe {
            // use write unaligned, userspace might not give us an aligned buffer
            let old_count = ptr::read_unaligned(self.dst_count_ptr);
            ptr::write_unaligned(self.dst_count_ptr, old_count + 1);
        }
    }

    fn get_capid_from_write_region(&mut self, region: &mut MemoryWriteRegion) -> Option<usize> {
        if self.remaining_cap_count == 0 {
            return None;
        }

        self.cap_id_current_read_count += region.read_bytes(
            &mut self.cap_id_buffer[self.cap_id_current_read_count..],
        );

        if self.cap_id_current_read_count == size_of::<usize>() {
            // we have finished reading an entire id
            self.cap_id_current_read_count = 0;
            self.remaining_cap_count -= 1;
            self.inc_copy_count();

            Some(usize::from_le_bytes(self.cap_id_buffer))
        } else {
            None
        }
    }
}