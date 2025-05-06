//! Provides utilites for writing debug text to the vga text buffer or the qemu debug port

use core::fmt::{self, Write};

use crate::arch::x64::*;
use crate::sync::IMutex;

/// Port number of the debug console in qemu
pub const DEBUGCON_PORT: u16 = 0xe9;

/// The writer for the qemu debug port, used by eprint!() and friends
pub static E_WRITER: IMutex<PortWriter> = IMutex::new(PortWriter::new(DEBUGCON_PORT));

/// Writes strings to a port
pub struct PortWriter {
    port: u16,
}

impl PortWriter {
    pub const fn new(port: u16) -> Self {
        PortWriter {
            port,
        }
    }

    pub fn write_byte(&self, byte: u8) {
        outb(self.port, byte);
    }

    pub fn write_string(&self, string: &str) {
        for b in string.bytes() {
            self.write_byte(b);
        }
    }
}

impl Write for PortWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// Prints to the qemu debug port
#[macro_export]
macro_rules! eprint {
	($($arg:tt)*) => ($crate::io::_eprint(format_args!($($arg)*)));
}

/// Prints to the qemu debug port
#[macro_export]
macro_rules! eprintln {
	() => ($crate::eprint!("\n"));
	($($arg:tt)*) => ($crate::eprint!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _eprint(args: fmt::Arguments) {
    E_WRITER.lock().write_fmt(args).unwrap();
}

/// Prints to the qemu debug port, but does not lock the writer, so it can always write, even from interrupt handlers
#[macro_export]
macro_rules! rprint {
	($($arg:tt)*) => ($crate::io::_rprint(format_args!($($arg)*)));
}

/// Prints to the qemu debug port, but does not lock the writer, so it can always write, even from interrupt handlers
#[macro_export]
macro_rules! rprintln {
	() => ($crate::rprint!("\n"));
	($($arg:tt)*) => ($crate::rprint!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _rprint(args: fmt::Arguments) {
    let mut writer = PortWriter::new(DEBUGCON_PORT);
    writer.write_fmt(args).unwrap();
}
