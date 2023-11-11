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
use alloc::vec::Vec;

use serde::{Serialize, Deserialize};

use aurora::prelude::*;
use aurora::process::{exit, Command};
use aser::from_bytes;
use sys::InitInfo;
use aurora::arpc;
use aurora::arpc::{arpc_interface, arpc_impl};

mod initrd;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    dprintln!("{}", info);

    exit();
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
struct Test2 {
    a: usize,
    b: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Test3 {
    a: usize,
}

#[arpc_interface(service_id = 0, name = "Add")]
trait AddService {
    fn add(&self, a: usize, b: usize) -> usize;
}

struct Adder;

#[arpc_impl]
impl AddService for Adder {
    fn add(&self, a: usize, b: usize) -> usize {
        a + b
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct New(Test);


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

    // this is rpc channel used to control fs server
    let (fs_client_endpoint, fs_server_endpoint) = arpc::make_endpoints()
        .expect("failed to make fs server rpc endpoints");

    dprintln!("starting fs server...");
    let fs_server = Command::from_bytes(initrd_info.fs_server.into())
        .named_arg("server_endpoint".to_owned(), &fs_server_endpoint)
        .spawn()
        .expect("failed to start fs server");


    let tmp = Test::D {
        bruh: 8,
        a: false,
        hi: 12309470182309128,
    };
    //let tmp = Test::B(69);
    //let tmp = Test::A;
    /*let result: Vec<u8> = aser::to_bytes(&New(tmp), 0).unwrap();
    dprintln!("test to bytes {:?}", result);
    let tmp: New = aser::from_bytes(&result).unwrap();
    dprintln!("test from bytes {:?}", tmp);
    let value: aser::Value = aser::from_bytes(&result).unwrap();
    dprintln!("value from bytes {:?}", value);
    let value2 = aser::Value::from_serialize(&tmp);
    dprintln!("value from test {:?}", value2);
    let test2: New = value.into_deserialize().unwrap();
    dprintln!("test from value {:?}", test2);*/

    /*let tmp = Test2 {
        a: 1,
        b: 69,
    };
    let result: Vec<u8> = aser::to_bytes(&tmp, 0).unwrap();
    let tmp2: Test3 = aser::from_bytes(&result).unwrap();
    dprintln!("{tmp2:?}");*/

    loop { core::hint::spin_loop(); }
}
