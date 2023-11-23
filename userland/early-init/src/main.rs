#![no_std]
#![no_main]

#![feature(naked_functions)]
#![feature(decl_macro)]
#![feature(trait_alias)]
#![feature(associated_type_defaults)]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use core::slice;

use aurora::prelude::*;
use aurora::process::{self, Command};
use aurora::thread;
use aser::from_bytes;
use initrd::InitrdData;
use sys::{InitInfo, MmioAllocator, Rsdp};
use fs_server::{Fs, FsAsync};
use hwaccess_server::{HwAccess, HwAccessAsync};

mod initrd;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    dprintln!("{}", info);

    process::exit();
}

#[naked]
#[no_mangle]
pub extern "C" fn _aurora_startup() {
    unsafe {
        asm!(
            "pop rdi", // process data pointer
            "pop rsi", // process data size
            "pop rdx", // startup data pointer
            "pop rcx", // startup data size
            "call _rust_startup",
            options(noreturn)
        )
    }
}

#[no_mangle]
pub extern "C" fn _rust_startup(
    process_data: *mut u8,
    process_data_size: usize,
    init_data: *mut u8,
    init_data_size: usize,
) -> ! {
    let process_data = unsafe {
        slice::from_raw_parts(process_data, process_data_size)
    };

    let (process_init_data, memory_entries) = aurora_core::process_data_from_slice(process_data)
        .expect("invalid process data array passed into program");

    aurora_core::init_allocation(process_init_data, memory_entries)
        .expect("failed to initialize aurora lib allocaror");

    let init_data = unsafe {
        slice::from_raw_parts(init_data, init_data_size)
    };

    let init_info: InitInfo = from_bytes(init_data)
        .expect("failed to deserialize init data");

    dprintln!("early-init started");

    // safety: we trust the kernel to give us a pointer to a valid initrd
    let initrd_info = unsafe {
        initrd::parse_initrd(init_info.initrd_address)
    };

    let hwaccess = start_hwaccess_server(&initrd_info, init_info.mmio_allocator, init_info.rsdp);
    let fs = start_fs_server(&initrd_info, &hwaccess);

    asynca::block_in_place(async move {
        //let result = fs.add(1, 2).await;
        //dprintln!("result: {result}");

        //let pci_devices = hwaccess.get_pci_devices().await;
        //dprintln!("devices: {pci_devices:x?}");
    });

    // can't use regular process exit here because that will terminate root thread group,
    // and kill every thread and process on the system
    thread::exit_thread_only();
}

fn start_hwaccess_server(initrd: &InitrdData, mmio: MmioAllocator, rsdp: Rsdp) -> HwAccess {
    let (hwaccess_client_endpoint, hwaccess_server_endpoint) = arpc::make_endpoints()
        .expect("failed to make hwaccess server rpc endpoints");

    dprintln!("starting hwaccess server...");
    let hwaccess_server = Command::from_bytes(initrd.hwaccess_server.into())
        .named_arg("server_endpoint".to_owned(), &hwaccess_server_endpoint)
        .named_arg("mmio_allocator".to_owned(), &mmio)
        .named_arg("rsdp".to_owned(), &rsdp)
        .spawn()
        .expect("failed to start hwaccess server");

    HwAccess::from(hwaccess_client_endpoint)
}

fn start_fs_server(initrd: &InitrdData, hwaccess: &HwAccess) -> Fs {
    // this is rpc channel used to control fs server
    let (fs_client_endpoint, fs_server_endpoint) = arpc::make_endpoints()
        .expect("failed to make fs server rpc endpoints");

    dprintln!("starting fs server...");
    let fs_server = Command::from_bytes(initrd.fs_server.into())
        .named_arg("server_endpoint".to_owned(), &fs_server_endpoint)
        .named_arg("hwaccess_server".to_owned(), hwaccess)
        .spawn()
        .expect("failed to start fs server");

    Fs::from(fs_client_endpoint)
}