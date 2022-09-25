use crate::sched::Registers;

pub mod idt;
pub mod pic;

#[no_mangle]
extern "C" fn rust_int_handler(int_num: u8, regs: &mut Registers, error_code: u64) -> bool {
    false
}

#[no_mangle]
extern "C" fn eoi() {}
