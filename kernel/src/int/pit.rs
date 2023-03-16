use core::time::Duration;
use core::convert::TryInto;

use crate::prelude::*;
use crate::arch::x64::*;
use crate::sync::IMutex;

const PIT_INTERRUPT_TERMINAL_COUNT: u8 = 0;
const PIT_ONE_SHOT: u8 = 1;
const PIT_RATE_GENERATOR: u8 = 2;
const PIT_SQUARE_WAVE: u8 = 3;
const PIT_SOFTWARE_STROBE: u8 = 4;
const PIT_HARDWARE_STROBE: u8 = 5;

const PIT_CHANNEL_0: u16 = 0x40;
const PIT_CHANNEL_1: u16 = 0x41;
const PIT_CHANNEL_2: u16 = 0x42;
const PIT_COMMAND: u16 = 0x43;

const NANOSEC_PER_CLOCK: u64 = 838;

pub static PIT: Pit = Pit::new();

/// Programmable interrupt timer
/// 
/// We only use this for calibrating the local apic timer, so it currently doesn't support regular timekeeping
pub struct Pit {
	// needed for certain operations
	lock: IMutex<()>,
	oneshot_callback: IMutex<fn() -> ()>
}

impl Pit {
	const fn new() -> Self {
		Pit {
			lock: IMutex::new(()),
			oneshot_callback: IMutex::new(||{}),
		}
	}

	// not safe to call from scheduler interrupt handler
	pub fn set_reset(&self, ticks: u16) {
		// channel 0, low - high byte, rate generator mode, 16 bit binary
		let _lock = self.lock.lock();
		outb(PIT_COMMAND, 0b00110100);
		outb(PIT_CHANNEL_0, get_bits(ticks as _, 0..8) as _);
		outb(PIT_CHANNEL_0, get_bits(ticks as _, 8..16) as _);
	}

	pub fn disable(&self) {
		let _lock = self.lock.lock();
		outb(PIT_COMMAND, 0b00110010);
	}

	// calls the given function after the specified duration
	// returns false if the duration given was too long
	// disables the pit after finishing
	pub unsafe fn one_shot(&self, duration: Duration, f: fn() -> ()) -> bool {
		let ticks = duration.as_nanos() as u64 / NANOSEC_PER_CLOCK;
		let ticks = match ticks.try_into() {
			Ok(ticks) => ticks,
			Err(_) => return false,
		};

		*self.oneshot_callback.lock() = f;

		self.set_reset(ticks);

		true
	}

	/// This is only called after one_shot, since pit does not support regular timekeeping
	pub fn irq_handler(&self) {
		self.oneshot_callback.lock()();
		self.disable();
	}
}
