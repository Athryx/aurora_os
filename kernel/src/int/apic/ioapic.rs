use crate::uses::*;
use core::ptr;
use modular_bitfield::{bitfield, BitfieldSpecifier};
use crate::int::idt::IRQ_TIMER;
use super::*;

#[derive(Debug, Clone, Copy)]
pub enum IoApicDest {
	To(u8),
	ToAll,
}

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
	select: *mut u32,
	reg: *mut u32,
	// holds max irqs - 1
	max_irq_index: u8,
}

impl IoApic {
	const IO_APIC_ID: u32 = 0;
	const IO_APIC_VER: u32 = 1;
	const IO_APIC_ARB: u32 = 2;

	// safety: have to call init before calling any other methods
	pub const unsafe fn new() -> Self {
		IoApic {
			select: null_mut(),
			reg: null_mut(),
			max_irq_index: 0,
		}
	}

	// safety: pass a valid address to from
	pub unsafe fn from(addr: PhysAddr) -> Self {
		let mut out = Self::new();
		out.init(addr);
		out
	}

	// safety: pass a valid address to init
	pub unsafe fn init(&mut self, addr: PhysAddr) {
		let addr = phys_to_virt(addr).as_u64() as usize;
		self.select = addr as *mut u32;
		self.reg = (addr + 0x10) as *mut u32;
		self.max_irq_index = get_bits(self.read_reg(Self::IO_APIC_VER) as usize, 16..24) as u8;
		for irq in 0..=self.max_irq_index {
			self.set_irq_entry(irq, IrqEntry::new_masked());
		}
	}

	fn irq_index(&self, irq: u8) -> Option<u32> {
		if irq > self.max_irq_index {
			None
		} else {
			Some(0x10 + irq as u32 * 2)
		}
	}

	// returns the number of irqs an apic has
	pub fn max_irq_index(&mut self) -> u8 {
		self.max_irq_index
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

	// returns true if succesfully set irq
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
