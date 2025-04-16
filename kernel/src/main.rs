#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(allocator_api)]
#![feature(stmt_expr_attributes)]
#![feature(const_mut_refs)]
#![feature(slice_index_methods)]
#![feature(slice_ptr_get)]
#![feature(dropck_eyepatch)]
#![feature(ptr_metadata)]
#![feature(let_chains)]
#![feature(try_blocks)]
#![feature(try_trait_v2)]
#![allow(dead_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

mod acpi;
mod alloc;
mod arch;
mod cap;
mod container;
mod event;
mod int;
mod mem;
mod sched;
mod sync;
mod syscall;
mod util;
mod vmem_manager;

mod consts;
mod config;
mod gdt;
mod gs_data;
mod io;
mod mb2;
mod prelude;
mod start_userspace;

use core::panic::PanicInfo;

use acpi::SdtType;
use arch::x64::*;
use consts::INIT_STACK;
use int::apic;
use mb2::BootInfo;
use gs_data::Prid;
use prelude::*;
use sched::kernel_stack::KernelStack;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rprintln!("{}", info);
    println!("{}", info);

    loop {
        cli();
        hlt();
    }
}

/// Initilizes all kernel subsystems, and starts all other cpu cores
///
/// Runs once on the startup core
fn init(boot_info_addr: usize) -> KResult<()> {
    // clear the vga text buffer
    io::WRITER.lock().clear();

    config_cpu_settings();

    let boot_info = unsafe { BootInfo::new(boot_info_addr) };

    let mmio_allocator = unsafe {
        alloc::init(&boot_info.memory_map)?
    };

    // initialize the cpu local data
    gs_data::init(Prid::from(0));

    // initalize the gdt from the gdt and tss stored in the cpu local data
    gdt::init();

    // load idt
    cpu_local_data().idt.load();

    syscall::init();

    // initialise the scheduler
    sched::init();
    sched::init_kernel_context().expect("failed to initialize kernel context");
    sched::init_cpu_local(*INIT_STACK)?;

    let acpi_madt = boot_info.rsdt.get_table(SdtType::Madt).unwrap();
    let madt = acpi_madt.assume_madt().unwrap();

    let ap_apic_ids = unsafe { apic::init_io_apic(&madt)? };
    unsafe {
        apic::init_local_apic();
    }

    // FIXME: this page faults
    //int::userspace_interrupt::init_interrupt_manager(ap_apic_ids.len() + 1)?;

    apic::smp_init(&ap_apic_ids)?;

    start_userspace::start_early_init_process(boot_info.initrd, mmio_allocator, boot_info.rsdp)
}

/// Rust entry point of the kernel on the startup core
///
/// Called by boot.asm
#[no_mangle]
pub extern "C" fn _start(boot_info_addr: usize) -> ! {
    bochs_break();

    init(boot_info_addr).expect("kernel init failed");

    println!("aurora v0.0.1");

    sti();

    #[cfg(test)]
    test_main();

    loop {
        hlt();
    }
}

/// Initializes ap cores
fn ap_init(id: usize, stack_addr: usize) -> KResult<()> {
    config_cpu_settings();

    // initialize the cpu local data
    gs_data::init(Prid::from(id));

    // initalize the gdt from the gdt and tss stored in the cpu local data
    gdt::init();

    // load idt
    cpu_local_data().idt.load();

    syscall::init();

    let stack_range = AVirtRange::new(
        VirtAddr::new(stack_addr + 8 - KernelStack::DEFAULT_SIZE),
        KernelStack::DEFAULT_SIZE,
    );
    sched::init_cpu_local(stack_range)?;

    unsafe {
        apic::init_local_apic();
    }

    apic::ap_init_finished();

    Ok(())
}

/// Rust entry point of kernel on ap cores
///
/// Called by ap_boot.asm
/// `id` is a unique id for each cpu core
/// `stack_top` is the virtual memory address of the current stack for the ap core
#[no_mangle]
pub extern "C" fn _ap_start(id: usize, stack_top: usize) -> ! {
    ap_init(id, stack_top).expect("ap init failed");

    eprintln!("ap {} started", id);

    sti();

    loop {
        hlt();
    }
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    eprintln!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
    eprintln!("All tests passed");
}

#[test_case]
fn test() {
    use alloc::{zm, PageAllocator};

    use mem::PageLayout;

    unsafe {
        let a1 = zm()
            .alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a2 = zm()
            .alloc(PageLayout::from_size_align_unchecked(2 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a3 = zm()
            .alloc(PageLayout::from_size_align_unchecked(2 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a4 = zm()
            .alloc(PageLayout::from_size_align_unchecked(10 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a5 = zm()
            .alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a6 = zm()
            .alloc(PageLayout::from_size_align_unchecked(15 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a7 = zm()
            .alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a8 = zm()
            .alloc(PageLayout::from_size_align_unchecked(1 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a9 = zm()
            .alloc(PageLayout::from_size_align_unchecked(5 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        eprintln!("{:x?}", a1);
        eprintln!("{:x?}", a2);
        eprintln!("{:x?}", a3);
        eprintln!("{:x?}", a4);
        eprintln!("{:x?}", a5);
        eprintln!("{:x?}", a6);
        eprintln!("{:x?}", a7);
        eprintln!("{:x?}", a8);
        eprintln!("{:x?}", a9);
        zm().dealloc(a9);
        zm().dealloc(a4);
        zm().dealloc(a1);
        zm().dealloc(a3);
        zm().dealloc(a8);
        zm().dealloc(a2);
        zm().dealloc(a5);
        zm().dealloc(a7);
        zm().dealloc(a6);

        let a1 = zm()
            .alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a2 = zm()
            .alloc(PageLayout::from_size_align_unchecked(2 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a3 = zm()
            .alloc(PageLayout::from_size_align_unchecked(2 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a4 = zm()
            .alloc(PageLayout::from_size_align_unchecked(10 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a5 = zm()
            .alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a6 = zm()
            .alloc(PageLayout::from_size_align_unchecked(15 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a7 = zm()
            .alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a8 = zm()
            .alloc(PageLayout::from_size_align_unchecked(1 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        let a9 = zm()
            .alloc(PageLayout::from_size_align_unchecked(5 * PAGE_SIZE, PAGE_SIZE))
            .unwrap();
        eprintln!("{:x?}", a1);
        eprintln!("{:x?}", a2);
        eprintln!("{:x?}", a3);
        eprintln!("{:x?}", a4);
        eprintln!("{:x?}", a5);
        eprintln!("{:x?}", a6);
        eprintln!("{:x?}", a7);
        eprintln!("{:x?}", a8);
        eprintln!("{:x?}", a9);
        zm().dealloc(a9);
        zm().dealloc(a4);
        zm().dealloc(a1);
        zm().dealloc(a3);
        zm().dealloc(a8);
        zm().dealloc(a2);
        zm().dealloc(a5);
        zm().dealloc(a7);
        zm().dealloc(a6);
    }

    eprintln!("tests done");
}
