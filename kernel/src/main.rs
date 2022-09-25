#![no_std]
#![no_main]
#![feature(maybe_uninit_uninit_array)]
#![feature(array_methods)]
#![feature(alloc_error_handler)]
#![feature(allocator_api)]
#![feature(stmt_expr_attributes)]
#![feature(const_mut_refs)]
#![feature(generic_associated_types)]
#![feature(bound_map)]
#![feature(slice_index_methods)]
#![feature(slice_ptr_len)]
#![feature(slice_ptr_get)]
#![feature(dropck_eyepatch)]
#![feature(ptr_metadata)]
/*#![feature(arc_new_cyclic)]
#![feature(const_btree_new)]
#![feature(alloc_prelude)]
#![feature(map_try_insert)]
#![feature(map_first_last)]*/
#![allow(dead_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod acpi;
mod alloc;
mod arch;
mod cap;
mod container;
mod int;
mod mem;
//mod vmem;
mod sched;
mod sync;
//mod time;

mod consts;
mod gs_data;
mod hwa_iter;
mod id;
mod io;
mod mb2;
mod misc;
mod prelude;
mod process;

use core::panic::PanicInfo;
use core::sync::atomic::AtomicUsize;

use acpi::SdtType;
use arch::x64::*;
use gs_data::{GsData, Prid};
use int::idt::Idt;
use mb2::BootInfo;
//use time::pit;
use prelude::*;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    eprintln!("{}", info);
    // Comment this out for now because for some reason it can cause panic loops
    //println!("cpu {}: {}", prid(), info);

    loop {
        cli();
        hlt();
    }
}

fn init(boot_info_addr: usize) -> KResult<()> {
    unsafe { mem::init(*consts::KERNEL_VMA) }

    io::WRITER.lock().clear();

    let boot_info = unsafe { BootInfo::new(boot_info_addr) };

    unsafe {
        alloc::init(&boot_info.memory_map)?;
    }

    gs_data::init(GsData {
        temp_syscall_return_rip: AtomicUsize::new(0),
        prid: Prid::from(0),
        idt: Idt::new(),
    });

    sched::init()?;

    //pit::disable();

    let acpi_madt = unsafe { boot_info.rsdt.get_table(SdtType::Madt).unwrap() };
    let madt = acpi_madt.assume_madt().unwrap();

    unsafe {
        //let ap_ids = apic::init(madt);
        //apic::smp_init(ap_ids, ap_code_zone, ap_addr_space);
    }

    Ok(())
}

// rust entry point of the kernel after boot.asm calls this
#[no_mangle]
pub extern "C" fn _start(boot_info_addr: usize) -> ! {
    bochs_break();

    init(boot_info_addr).expect("kernel init failed");

    println!("aurora v0.0.1");

    // TEMP
    eprintln!("{:?}", *crate::consts::AP_DATA);

    test();

    loop {
        hlt();
    }
}

// rust entry point on ap cores, called by ap_start.asm
#[no_mangle]
pub extern "C" fn _ap_start(id: usize, stack_top: usize) -> ! {
    loop {
        hlt();
    }
}

use alloc::{zm, PageAllocator};

use mem::PageLayout;

fn test() {
    unsafe {
        let a1 = zm().alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a2 = zm().alloc(PageLayout::from_size_align_unchecked(2 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a3 = zm().alloc(PageLayout::from_size_align_unchecked(2 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a4 = zm().alloc(PageLayout::from_size_align_unchecked(10 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a5 = zm().alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a6 = zm().alloc(PageLayout::from_size_align_unchecked(15 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a7 = zm().alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a8 = zm().alloc(PageLayout::from_size_align_unchecked(1 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a9 = zm().alloc(PageLayout::from_size_align_unchecked(5 * PAGE_SIZE, PAGE_SIZE)).unwrap();
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

        let a1 = zm().alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a2 = zm().alloc(PageLayout::from_size_align_unchecked(2 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a3 = zm().alloc(PageLayout::from_size_align_unchecked(2 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a4 = zm().alloc(PageLayout::from_size_align_unchecked(10 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a5 = zm().alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a6 = zm().alloc(PageLayout::from_size_align_unchecked(15 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a7 = zm().alloc(PageLayout::from_size_align_unchecked(4 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a8 = zm().alloc(PageLayout::from_size_align_unchecked(1 * PAGE_SIZE, PAGE_SIZE)).unwrap();
        let a9 = zm().alloc(PageLayout::from_size_align_unchecked(5 * PAGE_SIZE, PAGE_SIZE)).unwrap();
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
