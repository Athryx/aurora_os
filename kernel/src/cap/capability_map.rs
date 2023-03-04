use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{prelude::*, alloc::AllocRef};
use crate::container::HashMap;
use crate::process::Process;
use crate::sync::IMutex;

use super::{CapId, Capability};

type InnerCapMap<T> = IMutex<HashMap<CapId, Capability<T>>>;

/// A map that holds all the capabilities in a process
#[derive(Debug)]
pub struct CapabilityMap {
    next_id: AtomicUsize,
    process_map: InnerCapMap<Process>,
}

impl CapabilityMap {
    pub fn new(allocator: AllocRef) -> Self {
        CapabilityMap {
            next_id: AtomicUsize::new(0),
            process_map: IMutex::new(HashMap::new(allocator)),
        }
    }
}

macro_rules! generate_cap_methods {
    ($map:ty, $cap_type:ty, $cap_field:ident) => {
        impl $map {
            
        }
    };
}

generate_cap_methods!(CapabilityMap, Process, process_map);