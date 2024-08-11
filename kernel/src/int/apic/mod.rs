use core::sync::atomic::{AtomicUsize, Ordering, AtomicBool, AtomicU32};
use core::time::Duration;

use spin::Once;

use crate::alloc::PaRef;
use crate::arch::x64::{cpuid, io_wait};
use crate::int::pit::PIT_GLOBAL_SYSINT;
use crate::mem::PageLayout;
use crate::{config, consts};
use crate::int::apic::io_apic::IrqEntry;
use crate::int::{PIT_IRQ_SRC, PIT_TICK};
use crate::prelude::*;
use crate::sync::IMutex;
use crate::{acpi::madt::{Madt, MadtElem}, alloc::root_alloc_ref};
use crate::sched::kernel_stack::KernelStack;
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
pub unsafe fn init_io_apic(madt: &WithTrailer<Madt>) -> KResult<Vec<u8>> {
    assert!(cpuid::has_apic(), "apic support required");

    let mut local_apic_addr = PhysAddr::new(madt.data.lapic_addr as usize);

    let mut ap_apic_ids: Vec<u8> = Vec::new(root_alloc_ref());
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

    LOCAL_APIC_ADDR.store(local_apic_addr.as_usize(), Ordering::Release);

    // indicates the sytem has an 8259 pic that we have to disable
	// this is the only flags in flags, so I won't bother to make a bitflags for it
    if madt.data.lapic_flags & 1 > 0 {
        pic::disable();
    }

    // run second pass for IoAPicSrcOverride once we know the io apic is initialized
    for madt_entry in madt.iter() {
        if let MadtElem::IoApicSrcOverride(override_info) = madt_entry {
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

            // the only interrupt we care about from the pic is timer interrupt for calibrating local apic timer
            if override_info.irq_src == PIT_IRQ_SRC {
                let irq_entry = IrqEntry::from(PIT_TICK, IoApicDest::To(startup_core_apic_id), polarity, trigger_mode);

                io_apic().lock().set_irq_entry(override_info.global_sysint as u8, irq_entry);

                // store pit global sysint for disabling later
                PIT_GLOBAL_SYSINT.store(override_info.global_sysint as u8, Ordering::Release);
            }
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
    let mut local_apic = unsafe {
        LocalApic::new(PhysAddr::new(LOCAL_APIC_ADDR.load(Ordering::Acquire)))
    };

    local_apic.init_timer(crate::config::TIMER_PERIOD);

    cpu_local_data().set_local_apic(local_apic);
}

/// The number of remaining ap cores that need to finish up booting
static NUM_APS_TO_BOOT: AtomicUsize = AtomicUsize::new(0);

/// ap boot sequence is done, set to true to tell aps to start normal operations
static APS_GO: AtomicBool = AtomicBool::new(false);

/// Data structure that communicates information to ap boot assembly
#[repr(C)]
struct ApData {
    /// Atomic counter used to assign ids to ap cores
    id_counter: AtomicU32,
    padding: u32,
    /// Address to an array of stack pointers that the aps will use
    stacks: usize,
}

/// Initializes all other cpu cores
pub fn smp_init(ap_apic_ids: &[u8]) -> KResult<()> {
    let num_aps = ap_apic_ids.len();
    eprintln!("booting {} ap cores...", num_aps);

    NUM_APS_TO_BOOT.store(num_aps, Ordering::Release);
    config::set_cpu_count(num_aps + 1);

    if num_aps == 0 {
        // only 1 cpu core is present, no aps need to boot
        return Ok(())
    }

    let ap_code_src_virt_range = consts::AP_CODE_SRC_RANGE.to_virt();
    let mut ap_code_dest_virt_range = consts::AP_CODE_DEST_RANGE.to_virt();

    // copy ap code to the trampoline location
    unsafe {
        let ap_code_src = ap_code_src_virt_range.as_slice();
        let ap_code_dest = ap_code_dest_virt_range.as_slice_mut();

        ap_code_dest.copy_from_slice(ap_code_src);
    }

    // set up ap data
    let ap_data_offset = *consts::AP_DATA - *consts::AP_CODE_RUN_START;
	let ap_data = (ap_code_dest_virt_range.as_usize() + ap_data_offset) as *mut ApData;
	let ap_data = unsafe { ap_data.as_mut().unwrap() };
	ap_data.id_counter.store(1, Ordering::Release);

    let mut stacks = Vec::try_with_capacity(root_alloc_ref(), num_aps)?;
    for _ in 0..num_aps {
        // NOTE: this leaks memory on early return, shouldn't matter for now since we will panic on error
        let allocation = PaRef::zm().alloc(
            PageLayout::new_rounded(KernelStack::DEFAULT_SIZE, PAGE_SIZE).unwrap(),
        ).ok_or(SysErr::OutOfMem)?;
        
        stacks.push(allocation.addr() + allocation.size() - 8)?;
    }
    ap_data.stacks = stacks.as_ptr() as usize;

    let mut local_apic = cpu_local_data().local_apic();

    // start all ap cores
    // TODO: send init and startup ipis only to cores listed in the ap_ids vec
	local_apic.send_ipi(Ipi::Init(IpiDest::AllExcludeThis));

	io_wait(Duration::from_millis(1000));

	while NUM_APS_TO_BOOT.load(Ordering::Acquire) > 0 {
		local_apic.send_ipi(Ipi::Sipi(IpiDest::AllExcludeThis, (*consts::AP_CODE_RUN_START / 0x1000).try_into().unwrap()));
		io_wait(Duration::from_micros(200));
	}

	APS_GO.store(true, Ordering::Release);

    Ok(())
}

pub fn ap_init_finished() {
    NUM_APS_TO_BOOT.fetch_sub(1, Ordering::AcqRel);

    while !APS_GO.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
}