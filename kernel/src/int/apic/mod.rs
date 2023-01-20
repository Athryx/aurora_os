use core::sync::atomic::{AtomicUsize, Ordering};

use spin::Once;

use crate::arch::x64::cpuid;
use crate::int::apic::io_apic::IrqEntry;
use crate::int::IRQ_BASE;
use crate::prelude::*;
use crate::sync::IMutex;
use crate::{acpi::madt::{Madt, MadtElem}, alloc::root_alloc_ref};
use io_apic::{IoApic, IoApicDest};
use apic_modes::{PinPolarity, TriggerMode};
use super::pic;

mod apic_modes;
mod io_apic;
mod local_apic;

pub use local_apic::{LocalApic, Ipi, IpiDest};

// physical address of the local apic
static LOCAL_APIC_ADDR: AtomicUsize = AtomicUsize::new(0);

static IO_APIC: Once<IMutex<IoApic>> = Once::new();

fn io_apic() -> &'static IMutex<IoApic> {
    IO_APIC.get().expect("io apic has not been initialized")
}

/// Intializes the ioapic, the bootstrap cpu local apic, and disables the pic
/// 
/// Returns a vector of the apic ids of all ap cores to start up
/// 
/// # Safety
/// 
/// Must pass a valid madt
pub unsafe fn init_io_apic(madt: &Madt) -> KResult<Vec<u8>> {
    assert!(cpuid::has_apic(), "apic support required");

    let mut local_apic_addr = PhysAddr::new(madt.lapic_addr as usize);

    let mut ap_apic_ids: Vec<u8> = Vec::new(root_alloc_ref().downgrade());
    let startup_core_apic_id = cpuid::apic_id();

    for madt_entry in madt.iter() {
        match madt_entry {
            MadtElem::LocalApicOverride(info) => {
                local_apic_addr = PhysAddr::new(info.addr as usize)
            },
            // if either of the first 2 bits in flags are set, the cpu is able to be enabled
            MadtElem::ProcLocalApic(info) if info.flags & 0b11 > 0 => {
                if startup_core_apic_id != info.apic_id {
                    ap_apic_ids.push(info.apic_id)?;
                }
            },
            MadtElem::IoApic(io_apic_info) => {
                let global_sysint_base = io_apic_info.global_sysint_base;
                assert_eq!(global_sysint_base, 0, "only systems with 1 io apic are supported");

                let io_apic_addr = PhysAddr::new(io_apic_info.ioapic_addr as usize);
                IO_APIC.call_once(|| IMutex::new(unsafe { IoApic::new(io_apic_addr) }));
            },
            _ => (),
        }
    }

    assert!(IO_APIC.is_completed(), "could not find io apic");
    assert!(ap_apic_ids.len() < crate::config::MAX_CPUS, "too many cpus for os to use");

    LOCAL_APIC_ADDR.store(local_apic_addr.as_usize(), Ordering::Release);

    // indicates the sytem has an 8259 pic that we have to disable
	// this is the only flags in flags, so I won't bother to make a bitflags for it
    if madt.lapic_flags & 1 > 0 {
        pic::disable();
    }

    // run second pass for IoAPicSrcOverride once we know the io apic is initialized
    for madt_entry in madt.iter() {
        match madt_entry {
            MadtElem::IoApicSrcOverride(override_info) => {
                let polarity = match get_bits(override_info.flags as usize, 0..2) {
					0 => PinPolarity::default(),
					1 => PinPolarity::ActiveHigh,
					2 => panic!("invalid pin polarity flag in acpi tables"),
					3 => PinPolarity::ActiveLow,
					_ => unreachable!(),
				};

				let trigger_mode = match get_bits(override_info.flags as usize, 2..4) {
					0 => TriggerMode::default(),
					1 => TriggerMode::Edge,
					2 => panic!("invalid trigger mode flag in acpi tables"),
					3 => TriggerMode::Level,
					_ => unreachable!(),
				};

                let irq = override_info.irq_src + IRQ_BASE;
                let irq_entry = IrqEntry::from(irq, IoApicDest::To(startup_core_apic_id), polarity, trigger_mode);

                io_apic().lock().set_irq_entry(override_info.global_sysint as u8, irq_entry);
            },
            _ => (),
        }
    }

    Ok(ap_apic_ids)
}

/// Initialized the local apic
/// 
/// # Safety
/// 
/// Must be called after init_io_apic
pub unsafe fn init_local_apic() {
    let local_apic = unsafe {
        LocalApic::new(PhysAddr::new(LOCAL_APIC_ADDR.load(Ordering::Acquire)))
    };

    // do this before initializing apic timer so int::eoi can send eoi to the local apic
    cpu_local_data().set_local_apic(local_apic);

    cpu_local_data().local_apic().init_timer(crate::config::TIMER_PERIOD);
}