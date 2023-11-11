use core::panic::PanicInfo;

use crate::prelude::*;

#[lang = "eh_personality"]
#[no_mangle]
extern "C" fn rust_eh_personality() {}

#[lang = "panic_impl"]
#[no_mangle]
fn rust_begin_panic(info: &PanicInfo) -> ! {
	dprintln!("{}", info);

	aurora::process::exit();
}

/*#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}*/
