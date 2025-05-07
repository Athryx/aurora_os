//! Provides utilites for writing debug text to the vga text buffer or the qemu debug port

use core::fmt::{self, Write};

use lazy_static::lazy_static;
use log::{LevelFilter, Level, Log, Metadata, Record};
use volatile::Volatile;

use crate::arch::x64::*;
use crate::consts;
use crate::sync::IMutex;

const VGA_BUF_WIDTH: usize = 80;
const VGA_BUF_HEIGHT: usize = 25;

/// Port number of the debug console in qemu
pub const DEBUGCON_PORT: u16 = 0xe9;

lazy_static! {
    /// The writer for the vga text buffer, used by print!() and friends
    pub static ref WRITER: IMutex<Writer> = IMutex::new(Writer {
        xpos: 0,
        ypos: 0,
        color: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { ((*consts::KERNEL_VMA + 0xb8000) as *mut Buffer).as_mut().unwrap() },
    });
}

/// The writer for the qemu debug port, used by eprint!() and friends
pub static E_WRITER: IMutex<PortWriter> = IMutex::new(PortWriter::new(DEBUGCON_PORT));

/// Represents the vga text buffer
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; VGA_BUF_WIDTH]; VGA_BUF_HEIGHT],
}

/// A color code for the vga text buffer
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// A combination of 2 color codes representing the foreground and background color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(foreground: Color, background: Color) -> Self {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

// A character in the vga text buffer, has a glyph and a color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    cchar: u8,
    color: ColorCode,
}

impl ScreenChar {
    fn new(cchar: u8, color: ColorCode) -> Self {
        ScreenChar {
            cchar,
            color,
        }
    }
}

/// Writes characters to a buffer
pub struct Writer {
    xpos: usize,
    ypos: usize,
    color: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    // when this is called previous calls would have gauranteed xpos and ypos are correct
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.ypos += 1;
                self.xpos = 0;
                self.wrap_pos();
            },
            _ => {
                let ctow = ScreenChar::new(byte, self.color);
                self.buffer.chars[self.ypos][self.xpos].write(ctow);
                self.xpos += 1;
                self.wrap_pos();
            },
        }
    }

    pub fn write_string(&mut self, string: &str) {
        for b in string.bytes() {
            match b {
                0x20..=0x7e | b'\n' => self.write_byte(b),
                _ => self.write_byte(0xfe),
            }
        }
    }

    pub fn clear(&mut self) {
        for y in 0..VGA_BUF_HEIGHT {
            self.clear_row(y);
        }
    }

    fn scroll_down(&mut self, lines: usize) {
        if lines >= VGA_BUF_HEIGHT {
            for y in 0..VGA_BUF_HEIGHT {
                self.clear_row(y);
            }
            return;
        }

        for y in 0..(VGA_BUF_HEIGHT - lines) {
            for x in 0..VGA_BUF_WIDTH {
                let buf = &mut self.buffer.chars;
                buf[y][x].write(buf[y + lines][x].read());
            }
        }

        for y in (VGA_BUF_HEIGHT - lines)..VGA_BUF_HEIGHT {
            self.clear_row(y);
        }
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar::new(b' ', self.color);

        for x in 0..VGA_BUF_WIDTH {
            self.buffer.chars[row][x].write(blank);
        }
    }

    fn wrap_pos(&mut self) {
        if self.xpos >= VGA_BUF_WIDTH {
            self.xpos = 0;
            self.ypos += 1;
        }
        if self.ypos >= VGA_BUF_HEIGHT {
            self.scroll_down(self.ypos - VGA_BUF_HEIGHT + 1);
            self.ypos = VGA_BUF_HEIGHT - 1;
        }
    }
}

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// Prints to the vga text buffer
#[macro_export]
macro_rules! print {
	($($arg:tt)*) => ($crate::io::_print(format_args!($($arg)*)));
}

/// Prints to the vga text buffer
#[macro_export]
macro_rules! println {
	() => ($crate::print!("\n"));
	($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    WRITER.lock().write_fmt(args).unwrap();
}

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

struct Logger {
    log_level: LevelFilter,
    color: bool,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.log_level >= metadata.level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(&record.metadata()) {
            let mut writer = E_WRITER.lock();

            // add color for level text
            if self.color {
                let color = match record.level() {
                    Level::Error => "\x1b[0;31m",
                    Level::Warn => "\x1b[0;33m",
                    Level::Info => "\x1b[0;32m",
                    Level::Debug => "\x1b[0;34m",
                    Level::Trace => "\x1b[0;35m",
                };
                write!(writer, "{}", color).unwrap();
            }

            write!(writer, "{}", record.level()).unwrap();

            // make file location faint
            // colors at https://stackoverflow.com/questions/4842424/list-of-ansi-color-escape-sequences
            if self.color {
                write!(writer, "\x1b[38;5;242m").unwrap();
            }

            if let Some(line) = record.line() {
                write!(writer, " ({}:{}) ", record.target(), line).unwrap();
            } else {
                write!(writer, " ").unwrap();
            }

            // style log body based on log level
            if self.color {
                let color = match record.level() {
                    // error prints in all red
                    Level::Error => "\x1b[0;31m",
                    // trace prints fainter
                    Level::Trace => "\x1b[38;5;249m",
                    // everything else just prints default
                    _ => "\x1b[0m",
                };
                write!(writer, "{}", color).unwrap();
            }

            writer.write_fmt(*record.args()).unwrap();

            write!(writer, "\n").unwrap();

            // remove color (in case other things are printed after)
            if self.color {
                write!(writer, "\x1b[0m").unwrap();
            }
        }
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger {
    log_level: LevelFilter::Trace,
    color: true,
};

pub fn init_logging() {
    log::set_max_level(LOGGER.log_level);
    log::set_logger(&LOGGER).expect("failed to set logger");
}