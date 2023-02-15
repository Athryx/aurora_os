use core::char::MAX;
use core::time::Duration;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_CPUS: usize = 16;

/// How long between interrupts on local apic timer
pub const TIMER_PERIOD: Duration = Duration::from_millis(2);

/// How long the scheduler will wait before switching threads
pub const SCHED_TIME: Duration = Duration::from_millis(10);

static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn set_cpu_count(cpu_count: usize) {
    assert!(cpu_count <= MAX_CPUS, "there are too many cpus for os to use");
    CPU_COUNT.store(cpu_count, Ordering::Release);
}

pub fn cpu_count() -> usize {
    CPU_COUNT.load(Ordering::Acquire)
}