use core::sync::atomic::{AtomicU64, Ordering};

use crate::alloc::HeapRef;
use crate::cap::key::Key;
use crate::cap::{CapObject, CapType};
use crate::prelude::*;
use crate::container::{Arc, Weak};
use crate::sync::IMutex;
use super::Process;

/// Specifies the key id needed to create spawners
static SPAWNER_KEY_ID: AtomicU64 = AtomicU64::new(0);

/// Capability that allows spawning processess, and manages destroying process groups
#[derive(Debug)]
pub struct Spawner {
    process_list: IMutex<Vec<Weak<Process>>>,
}

impl Spawner {
    pub fn new(allocer: HeapRef) -> Self {
        Spawner {
            process_list: IMutex::new(Vec::new(allocer)),
        }
    }

    /// Adds the process to this spawner
    pub fn add_process(&self, process: Weak<Process>) -> KResult<()> {
        self.process_list.lock().push(process)
    }

    /// Kills all processess that the spawner currently has that are not the currently running process
    /// 
    /// Returns the currently running process if this spawner has it, or None if it doesn't
    /// 
    /// The caller than must kill the current process at an appropriate time
    pub fn kill_all_processes(&self) -> Option<Arc<Process>> {
        let mut process_list = self.process_list.lock();

        // store the current process if we encounter it
        // we cannot hold any resources so other code has to terminate
        // the current process when the current thread releases all resources
        let mut current_process = None;

        while let Some(process) = process_list.pop() {
            // ignore processess that have been dropped
            if let Some(process) = process.upgrade() {
                if process.is_current_process() {
                    current_process = Some(process);
                } else {
                    Process::exit(process);
                }
            }
        }

        current_process
    }

    /// Gets the id of the key needed to create spawners
    pub fn key_id() -> u64 {
        SPAWNER_KEY_ID.load(Ordering::Acquire)
    }

    /// Sets the id of the key needed to create spawners
    pub fn set_key_id(key: Key) {
        SPAWNER_KEY_ID.store(key.id(), Ordering::Release)
    }
}

impl CapObject for Spawner {
    const TYPE: CapType = CapType::Spawner;
}