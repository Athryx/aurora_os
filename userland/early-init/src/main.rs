#![no_std]
#![no_main]

#![feature(naked_functions)]
#![feature(async_fn_in_trait)]
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
use sys::InitInfo;
use fs_server::{Fs, FsAsync};

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

    // this is rpc channel used to control fs server
    let (fs_client_endpoint, fs_server_endpoint) = arpc::make_endpoints()
        .expect("failed to make fs server rpc endpoints");

    dprintln!("starting fs server...");
    let fs_server = Command::from_bytes(initrd_info.fs_server.into())
        .named_arg("server_endpoint".to_owned(), &fs_server_endpoint)
        .spawn()
        .expect("failed to start fs server");

    let fs_client = Fs::from(fs_client_endpoint);

    asynca::block_in_place(async move {
        let result = fs_client.add(1, 2).await;
        dprintln!("result: {result}");
    });

    // can't use regular process exit here because that will terminate root thread group,
    // and kill every thread and process on the system
    thread::exit_thread_only();
}
