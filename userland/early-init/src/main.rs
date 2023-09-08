#![no_std]
#![no_main]

#![feature(naked_functions)]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use core::slice;
use alloc::vec::Vec;

use serde::{Serialize, Deserialize};

use aurora::prelude::*;
use aurora::process::Command;
use aser::from_bytes;
use sys::InitInfo;

mod initrd;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    dprintln!("{}", info);

    loop { core::hint::spin_loop(); }
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Test {
    A,
    B(i32),
    C(u8, u8),
    D {
        bruh: u8,
        a: bool,
        hi: i128,
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

    let (process_init_data, memory_entries) = aurora::process_data_from_slice(process_data)
        .expect("invalid process data array passed into program");

    aurora::init_allocation(process_init_data, memory_entries)
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

    dprintln!("starting fs server...");
    let fs_server = Command::from_bytes(initrd_info.fs_server.into())
        .spawn()
        .expect("failed to start fs server");


    let tmp = Test::D {
        bruh: 8,
        a: false,
        hi: 12309470182309128,
    };
    //let tmp = Test::B(69);
    //let tmp = Test::A;
    let result: Vec<u8> = aser::to_bytes(&tmp, 0).unwrap();
    dprintln!("test to bytes {:?}", result);
    let tmp: Test = aser::from_bytes(&result).unwrap();
    dprintln!("test from bytes {:?}", tmp);
    let value: aser::Value = aser::from_bytes(&result).unwrap();
    dprintln!("value from bytes {:?}", value);
    let value2 = aser::Value::from_serialize(&tmp);
    dprintln!("value from test {:?}", value2);
    let test2: Test = value.into_deserialize().unwrap();
    dprintln!("test from value {:?}", test2);

    loop { core::hint::spin_loop(); }
}
