#![no_std]
#![no_main]

#![feature(asm)]
#![feature(const_fn_trait_bound)]
#![feature(maybe_uninit_uninit_array)]
#![feature(array_methods)]
#![feature(alloc_error_handler)]
#![feature(allocator_api)]
#![feature(stmt_expr_attributes)]
#![feature(const_mut_refs)]
#![feature(generic_associated_types)]
#![feature(bound_map)]
#![feature(slice_index_methods)]

/*#![feature(arc_new_cyclic)]
#![feature(const_btree_new)]
#![feature(alloc_prelude)]
#![feature(map_try_insert)]
#![feature(map_first_last)]*/

#![allow(dead_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod alloc;
mod arch;
mod acpi;
mod container;
mod mem;
mod sync;

mod consts;
mod hwa_iter;
mod misc;
mod mb2;
mod io;
mod prelude;

use core::panic::PanicInfo;

use prelude::*;
use arch::x64::*;
use mb2::BootInfo;

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
	unsafe {
		mem::init(*consts::KERNEL_VMA)
	}

	io::WRITER.lock().clear();

	let boot_info = unsafe { BootInfo::new(boot_info_addr) };

	alloc::init(&boot_info.memory_map);

	Ok(())
}

// rust entry point of the kernel after boot.asm calls this
#[no_mangle]
pub extern "C" fn _start(boot_info_addr: usize) -> ! {
	bochs_break();

	init(boot_info_addr).expect("kernel init failed");

	println!("aurora v0.0.1");

	loop {
		hlt();
	}
}
