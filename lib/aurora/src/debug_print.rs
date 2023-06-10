use core::fmt::{self, Write};

use spin::mutex::Mutex;
use sys::debug_print;

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
    ($($arg:tt)*) => ($crate::debug_print::_dprint(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! dprintln {
    () => ($crate::dprint!("\n"));
    ($($arg:tt)*) => ($crate::dprint!("{}\n", format_args!($($arg)*)));
}