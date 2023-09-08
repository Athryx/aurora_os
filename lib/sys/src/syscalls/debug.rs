use core::cmp::min;
use core::fmt::{self, Write};

use spin::Mutex;

use crate::{syscall_nums::*, syscall};

/// Prints up to 64 bytes from the input array to the kernel debug log
fn print_debug_inner(data: &[u8]) {
    let num_chars = min(64, data.len());

    let get_char = |n| *data.get(n).unwrap_or(&0) as usize;

    let get_arg = |arg: usize| {
        let base = arg * 8;

        get_char(base)
            | get_char(base + 1) << 8
            | get_char(base + 2) << 16
            | get_char(base + 3) << 24
            | get_char(base + 4) << 32
            | get_char(base + 5) << 40
            | get_char(base + 6) << 48
            | get_char(base + 7) << 56
    };

    unsafe {
        syscall!(
            PRINT_DEBUG,
            num_chars,
            get_arg(0),
            get_arg(1),
            get_arg(2),
            get_arg(3),
            get_arg(4),
            get_arg(5),
            get_arg(6),
            get_arg(7)
        );
    }
}

/// Prints `data` to the kernel debug log
pub fn debug_print(data: &[u8]) {
    for chunk in data.chunks(64) {
        print_debug_inner(chunk);
    }
}

/// A writer which writes output to the debug_print syscall
struct DebugWriter;

impl Write for DebugWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        debug_print(s.as_bytes());
        Ok(())
    }
}

static DEBUG_WRITER: Mutex<DebugWriter> = Mutex::new(DebugWriter);

#[doc(hidden)]
pub fn _dprint(args: fmt::Arguments) {
    DEBUG_WRITER.lock().write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! dprint {
    ($($arg:tt)*) => ($crate::_dprint(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! dprintln {
    () => ($crate::dprint!("\n"));
    ($($arg:tt)*) => ($crate::dprint!("{}\n", format_args!($($arg)*)));
}