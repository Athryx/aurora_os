#![no_std]
#![no_main]

#![feature(naked_functions)]

use core::arch::asm;
use core::panic::PanicInfo;
use core::slice;

use aurora::dprintln;
use aser::from_bytes;
use sys::InitInfo;

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

#[no_mangle]
pub extern "C" fn _rust_startup(
    process_data: *mut u8,
    process_data_size: usize,
    startup_data: *mut u8,
    startup_data_size: usize,
) -> ! {
    let process_data = unsafe {
        slice::from_raw_parts(process_data, process_data_size)
    };

    let (process_init_data, memory_entries) = aurora::process_data_from_slice(process_data)
        .expect("invalid process data array passed into program");

    aurora::init_allocation(process_init_data, memory_entries)
        .expect("failed to initialize aurora lib allocaror");

    let startup_data = unsafe {
        slice::from_raw_parts(startup_data, startup_data_size)
    };

    let init_info: InitInfo = from_bytes(startup_data)
        .expect("failed to deserialize startup data");

    dprintln!("early-init started");

    loop { core::hint::spin_loop(); }
}
