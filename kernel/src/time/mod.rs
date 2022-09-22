pub mod pit;
pub mod apic_timer;

pub use apic_timer::{ApicTimer, apic_timer};
pub use core::time::Duration;

pub trait Timer {
	fn nsec(&self) -> u64;

	fn nsec_no_latch(&self) -> u64 {
		self.nsec()
	}

	fn duration(&self) -> Duration {
		Duration::from_nanos(self.nsec())
	}

	fn duration_no_latch(&self) -> Duration {
		Duration::from_nanos(self.nsec_no_latch())
	}
}

pub fn timer() -> &'static ApicTimer {
	&apic_timer
}

pub const NANOSEC_PER_SEC: u64 = 1000000000;
