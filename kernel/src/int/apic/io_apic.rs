use core::ptr;

use modular_bitfield::{bitfield, specifiers::*};

use crate::prelude::*;
use super::apic_modes::*;

#[derive(Debug, Clone, Copy)]
pub enum IoApicDest {
	To(u8),
	ToAll,
}

/// Specifies how a hardware interrupt sent to the io apic is delivered to local apics
#[bitfield]
#[repr(u64)]
#[derive(Debug, Clone, Copy)]
pub struct IrqEntry {
	vec: u8,

	#[bits = 3]
	deliv_mode: DelivMode,

	#[bits = 1]
	dest_mode: DestMode,

	// read only
	#[bits = 1]
	#[skip(setters)]
	deliv_status: DelivStatus,

	#[bits = 1]
	polarity: PinPolarity,

	// read only
	#[bits = 1]
	#[skip(setters)]
	remote_irr: RemoteIrr,

	#[bits = 1]
	trigger_mode: TriggerMode,

	masked: bool,

	#[skip] __: B39,

	dest: u8,
}

impl IrqEntry {
	pub(super) fn from(vec: u8, dest: IoApicDest, polarity: PinPolarity, trigger_mode: TriggerMode) -> Self {
		let out = Self::new()
			.with_deliv_mode(DelivMode::Fixed)
			.with_vec(vec)
			.with_polarity(polarity)
			.with_trigger_mode(trigger_mode);

		match dest {
			IoApicDest::To(dest) => out.with_dest_mode(DestMode::Physical).with_dest(dest),
			IoApicDest::ToAll => out.with_dest_mode(DestMode::Logical).with_dest(1),
		}
	}

	pub fn new_masked() -> Self {
		Self::new().with_masked(true)
	}
}

pub struct IoApic {
    /// Memory region to select which register to read and write to
	select: *mut u32,
    /// Memory region to actually read and write to register once a register is selected
	reg: *mut u32,
	/// Maximum valid irq index
	max_irq_index: u8,
}

impl IoApic {
	const IO_APIC_ID_REG: u32 = 0;
	const IO_APIC_VERSION_REG: u32 = 1;
	const IO_APIC_ARBITRATION_REG: u32 = 2;

    /// Creates a new IoApic
    /// 
    /// # Safety
    /// 
    /// `addr` must point to a valid io apic memory region
    pub unsafe fn new(addr: PhysAddr) -> Self {
        let addr = addr.to_virt().as_usize();

        let mut out = IoApic {
            select: addr as *mut u32,
            reg: (addr + 0x10) as *mut u32,
            max_irq_index: 0,
        };

        out.max_irq_index = get_bits(out.read_reg(Self::IO_APIC_VERSION_REG) as usize, 16..24) as u8;

        out
    }

    /// Returns the base register index of the given irq, or none if it is an invalid irq
	fn irq_index(&self, irq: u8) -> Option<u32> {
		if irq > self.max_irq_index {
			None
		} else {
			Some(0x10 + irq as u32 * 2)
		}
	}

	fn read_reg(&mut self, reg: u32) -> u32 {
		unsafe {
			ptr::write_volatile(self.select, reg);
			ptr::read_volatile(self.reg)
		}
	}

	fn write_reg(&mut self, reg: u32, data: u32) {
		unsafe {
			ptr::write_volatile(self.select, reg);
			ptr::write_volatile(self.reg, data);
		}
	}

	/// Sets the given irq to the irq entry, returns true on success
	pub fn set_irq_entry(&mut self, irq: u8, entry: IrqEntry) -> bool {
		match self.irq_index(irq) {
			Some(index) => {
				let entry: u64 = entry.into();
				self.write_reg(index, get_bits(entry as usize, 0..32) as u32);
				self.write_reg(index + 1, get_bits(entry as usize, 32..64) as u32);
				true
			},
			None => false,
		}
	}
}

unsafe impl Send for IoApic {}
