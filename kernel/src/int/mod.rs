use crate::sched::Registers;

pub mod idt;
pub mod pic;

/// Called by each assembly interrupt handler
/// 
/// Returns true to indicate if registers have changed and should be reloaded
#[no_mangle]
extern "C" fn rust_int_handler(int_num: u8, regs: &mut Registers, error_code: u64) -> bool {
    match int_num {
        _ => false,
    }
}

#[no_mangle]
extern "C" fn eoi() {}
