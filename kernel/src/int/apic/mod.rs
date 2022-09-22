use crate::prelude::*;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use core::convert::TryInto;
use core::slice;
use core::time::Duration;
use alloc::collections::BTreeMap;
use modular_bitfield::BitfieldSpecifier;
use crate::mem::virt_alloc::{VirtMapper, VirtLayoutElement, VirtLayout, PageMappingFlags, AllocType};
use crate::mem::{VirtRange, PAGE_SIZE};
use crate::mem::phys_alloc::{zm, Allocation, ZoneManager};
use crate::config::{MAX_CPUS, TIMER_PERIOD, set_cpu_count};
use crate::acpi::madt::{Madt, MadtElem};
use crate::arch::x64::io_wait;
use crate::sched::{sleep, Stack};
use crate::sync::IMutex;
use crate::int::idt::{irq_arr, IRQ_BASE, IRQ_TIMER};
use crate::consts::{AP_PHYS_START, AP_CODE_START, AP_CODE_END, AP_DATA};
use super::pic;

pub mod lapic;
pub mod ioapic;

use ioapic::{IrqEntry, IoApicDest};
use lapic::{Ipi, IpiDest};

pub use lapic::LocalApic;
pub use ioapic::IoApic;

// used to tell ap cores where their apic is
pub static LAPIC_ADDR: AtomicUsize = AtomicUsize::new(0);
pub static BSP_ID: AtomicU8 = AtomicU8::new(0);
pub static IO_APIC: IMutex<IoApic> = IMutex::new(unsafe { IoApic::new() });

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
#[bits = 3]
enum DelivMode {
	Fixed = 0,
	// only available for io apic and ipi
	// avoid for ipi
	LowPrio = 1,
	// avoid for ipi
	Smi = 2,
	Nmi = 4,
	Init = 5,
	// only available for ipi
	Sipi = 6,
	ExtInt = 7,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
enum DestMode {
	Physical = 0,
	Logical = 1,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
enum DelivStatus {
	Idle = 0,
	Pending = 1,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
enum TriggerMode {
	Edge = 0,
	// avoid for ipi
	Level = 1,
}

// Default for when acpi tables say use default
impl Default for TriggerMode {
	fn default() -> Self {
		Self::Edge
	}
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
enum PinPolarity {
	ActiveHigh = 0,
	ActiveLow = 1,
}

// Default for when acpi tables say use default
impl Default for PinPolarity {
	fn default() -> Self {
		Self::ActiveHigh
	}
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
enum RemoteIrr {
	None = 0,
	Servicing = 1,
}

#[derive(Debug, Clone, Copy)]
struct IrqOverride {
	sysint: u32,
	polarity: PinPolarity,
	trigger_mode: TriggerMode,
}

#[derive(Debug)]
struct IrqOverrides {
	map: BTreeMap<u8, IrqOverride>,
}

impl IrqOverrides {
	const fn new() -> Self {
		IrqOverrides {
			map: BTreeMap::new(),
		}
	}

	fn get_irq(&self, irq: u8) -> IrqOverride {
		if let Some(irq) = self.map.get(&irq) {
			*irq
		} else {
			IrqOverride {
				sysint: irq as u32,
				polarity: PinPolarity::default(),
				trigger_mode: TriggerMode::default(),
			}
		}
	}

	fn override_irq(&mut self, irq: u8, over: IrqOverride) {
		self.map.insert(irq, over);
	}
}

// FIXME: correctly handle global sysint
pub unsafe fn init(madt: &Madt) -> Vec<u8> {
	let mut lapic_addr = madt.lapic_addr as usize;

	// indicates the sytem has an 8259 pic that we have to disable
	// this is the only flags in flags, so I won't bother to make a bitflags for it
	if madt.lapic_flags & 1 > 0 {
		pic::disable();
	}
	
	// store irq overrides to make sure io apic is initialized first
	let mut overrides = IrqOverrides::new();

	// ap lapic ids
	let mut ap_ids = Vec::new();

	// if the bsp ProcLocalApic has been encountered yet
	let mut flag = true;

	for entry in madt.iter() {
		match entry {
			MadtElem::ProcLocalApic(data) => {
				if flag {
					BSP_ID.store(data.apic_id, Ordering::Release);
					flag = false;
				} else {
					ap_ids.push(data.apic_id);
				}
			}
			MadtElem::IoApic(io_apic) => {
				// to avoid warning about refernce to packed field
				let sysint = io_apic.global_sysint_base;
				// this will only be non zero in systems with multiple apics, which we do not support
				assert_eq!(sysint, 0);

				let ioapic_addr = PhysAddr::new(io_apic.ioapic_addr as u64);
				IO_APIC.lock().init(ioapic_addr);
			}
			MadtElem::IoApicSrcOverride(data) => {
				let polarity = match get_bits(data.flags as usize, 0..2) {
					0 => PinPolarity::default(),
					1 => PinPolarity::ActiveHigh,
					2 => panic!("invalid pin polarity flag in acpi tables"),
					3 => PinPolarity::ActiveLow,
					_ => unreachable!(),
				};

				let trigger_mode = match get_bits(data.flags as usize, 0..2) {
					0 => TriggerMode::default(),
					1 => TriggerMode::Edge,
					2 => panic!("invalid trigger mode flag in acpi tables"),
					3 => TriggerMode::Level,
					_ => unreachable!(),
				};

				let over = IrqOverride {
					sysint: data.global_sysint,
					polarity,
					trigger_mode,
				};

				overrides.override_irq(data.irq_src + IRQ_BASE, over);
			},
			MadtElem::LocalApicOverride(data) => lapic_addr = data.addr as usize,
			_ => (),
		}
	}

	assert!(ap_ids.len() < MAX_CPUS);

	let mut io_apic = IO_APIC.lock();

	for irq in irq_arr() {
		let over = overrides.get_irq(irq);
		let entry = if irq == IRQ_TIMER {
			IrqEntry::from(irq, IoApicDest::To(0), over.polarity, over.trigger_mode)
		} else {
			IrqEntry::from(irq, IoApicDest::To(BSP_ID.load(Ordering::Acquire)), over.polarity, over.trigger_mode)
		};
		io_apic.set_irq_entry(over.sysint as u8, entry);
	}

	drop(io_apic);

	LAPIC_ADDR.store(lapic_addr, Ordering::Release);

	let mut lapic = LocalApic::from(PhysAddr::new(lapic_addr as u64));
	lapic.init_timer(TIMER_PERIOD);
	cpud().set_lapic(lapic);

	ap_ids
}

#[repr(C)]
#[derive(Debug)]
struct ApData {
	cr3: u32,
	idc: u32,
	stacks: usize,
}

static APS_TO_BOOT: AtomicUsize = AtomicUsize::new(0);
static APS_GO: AtomicBool = AtomicBool::new(false);

pub unsafe fn smp_init(ap_ids: Vec<u8>, mut ap_code_zone: Allocation, ap_addr_space: VirtMapper<ZoneManager>) {
	APS_TO_BOOT.store(ap_ids.len(), Ordering::Release);
	set_cpu_count(ap_ids.len() + 1);

	let ap_bin_start = phys_to_virt_usize(*AP_PHYS_START);
	let ap_virt_start = phys_to_virt_usize(*AP_CODE_START);
	let ap_code_size = *AP_CODE_END - *AP_CODE_START;
	let ap_data_offset = *AP_DATA - *AP_CODE_START;

	// copy code to ap code zone
	let ap_code = slice::from_raw_parts(ap_bin_start as *const u8, ap_code_size);
	ap_code_zone.copy_from_mem(ap_code);

	// map ap code zone in ap addr space
	let velem = VirtLayoutElement::from_mem(ap_code_zone, PAGE_SIZE, PageMappingFlags::READ | PageMappingFlags::WRITE | PageMappingFlags::EXEC);
	let vlayout = VirtLayout::from(vec![velem], AllocType::VirtMem);
	let vzone = VirtRange::new(VirtAddr::new(*AP_CODE_START as u64), PAGE_SIZE);
	ap_addr_space.map_at(vlayout, vzone).unwrap();

	// set up ap data
	let ap_data = (ap_virt_start + ap_data_offset) as *mut ApData;
	let ap_data = ap_data.as_mut().unwrap();
	// this lossy as cast is ok because ap addr space cr3 is guarenteed to be bellow 4 GiB
	ap_data.cr3 = ap_addr_space.get_cr3() as u32;
	ap_data.idc = 1;

	let mut stacks = vec![0; ap_ids.len()];
	for stack in stacks.iter_mut() {
		let addr = zm.alloc(Stack::DEFAULT_KERNEL_SIZE).unwrap().as_usize();
		*stack = addr + Stack::DEFAULT_KERNEL_SIZE;
	}
	ap_data.stacks = stacks.as_ptr() as usize;

	let mut cpd = cpud();
	let lapic = cpd.lapic();

	// TODO: send init and startup ipis only to cores listed in the ap_ids vec
	lapic.send_ipi(Ipi::Init(IpiDest::AllExcludeThis));

	io_wait(Duration::from_millis(1000));

	while APS_TO_BOOT.load(Ordering::Acquire) > 0 {
		lapic.send_ipi(Ipi::Sipi(IpiDest::AllExcludeThis, (*AP_CODE_START / 0x1000).try_into().unwrap()));
		io_wait(Duration::from_micros(200));
	}

	APS_GO.store(true, Ordering::Release);
}

// called by aps to initialize their local apics
pub fn ap_init() {
	let lapic_addr = LAPIC_ADDR.load(Ordering::Acquire);

	unsafe {
		let mut lapic = LocalApic::from(PhysAddr::new(lapic_addr as u64));
		lapic.init_timer(TIMER_PERIOD);
		cpud().set_lapic(lapic);
	}

	APS_TO_BOOT.fetch_sub(1, Ordering::AcqRel);

	while !APS_GO.load(Ordering::Acquire) {
		core::hint::spin_loop();
	}
}
