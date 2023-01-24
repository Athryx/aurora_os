use core::char::MAX;
use core::time::Duration;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_CPUS: usize = 16;

pub const TIMER_PERIOD: Duration = Duration::from_millis(40);

static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn set_cpu_count(cpu_count: usize) {
    assert!(cpu_count <= MAX_CPUS, "there are too many cpus for os to use");
    CPU_COUNT.store(cpu_count, Ordering::Release);
}

pub fn cpu_count() -> usize {
    CPU_COUNT.load(Ordering::Acquire)
}