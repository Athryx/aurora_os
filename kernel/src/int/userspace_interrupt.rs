use spin::Once;
use array_init::array_init;
use sys::{CapType, EventData, InterruptTrigger};

use crate::alloc::root_alloc_ref;
use crate::gs_data::Prid;
use crate::{alloc::HeapRef, sync::IMutexGuard};
use crate::event::{BroadcastEventEmitter, BroadcastEventListener};
use crate::prelude::*;
use crate::cap::CapObject;
use crate::container::Arc;
use crate::sync::IMutex;
use super::{USER_INTERRUPT_COUNT, USER_INTERRUPT_START};

type InterruptEventEmmiter = IMutex<BroadcastEventEmitter>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptId {
    pub cpu: Prid,
    pub interrupt_num: u8,
}

/// The interrupt manager says where each userspace interrupt on a given cpu and interrupt vector,
/// which capability the interrupt event should be sent to
pub struct InterruptManager {
    // the cpu the next interrupt will be allocated on
    // this is to try and spread interrupt handling out among cpus
    next_alloc_cpu: usize,
    interrupts: Vec<[Option<Arc<InterruptEventEmmiter>>; USER_INTERRUPT_COUNT]>,
}

impl InterruptManager {
    fn new(allocator: HeapRef, num_cpus: usize) -> KResult<Self> {
        let mut interrupts = Vec::try_with_capacity(allocator, num_cpus)?;

        for _ in 0..num_cpus {
            interrupts.push(array_init(|_| None))?;
        }

        Ok(InterruptManager {
            next_alloc_cpu: 0,
            interrupts,
        })
    }

    fn get_int_entry(&self, interrupt_id: InterruptId) -> &Option<Arc<InterruptEventEmmiter>> {
        &self.interrupts[interrupt_id.cpu.into()][(interrupt_id.interrupt_num - USER_INTERRUPT_START) as usize]
    }

    fn get_int_entry_mut(&mut self, interrupt_id: InterruptId) -> &mut Option<Arc<InterruptEventEmmiter>> {
        &mut self.interrupts[interrupt_id.cpu.into()][(interrupt_id.interrupt_num - USER_INTERRUPT_START) as usize]
    }

    fn inc_next_alloc_cpu(&mut self) {
        self.next_alloc_cpu += 1;
        if self.next_alloc_cpu == self.interrupts.len() {
            self.next_alloc_cpu = 0;
        }
    }

    /// Triggers an interrupt event to be emmitted for the given interrupt
    pub fn notify_interrupt(&self, interrupt_id: InterruptId) -> KResult<()> {
        if let Some(interrupt) = self.get_int_entry(interrupt_id) {
            interrupt.lock().emit_event(EventData::InterruptTrigger(InterruptTrigger))
        } else {
            Ok(())
        }
    }

    /// Creates a new interrupt emmitter at a given interrupt id
    // TODO: make this function faster, currently it is O(n)
    // where n is the number of possible interrupt ids
    fn create_interrupt(&mut self, allocator: &HeapRef) -> KResult<(InterruptId, Arc<InterruptEventEmmiter>)> {
        let first_iter = self.interrupts[self.next_alloc_cpu..].iter().enumerate();
        let second_iter = self.interrupts[..self.next_alloc_cpu].iter().enumerate();

        let mut interrupt_id = InterruptId {
            cpu: Prid::from(self.next_alloc_cpu),
            // TODO: don't always use interrupt 0
            interrupt_num: 0,
        };

        'outer: for (cpu_num, cpu_ints) in first_iter.chain(second_iter) {
            for (int_num, interrupt) in cpu_ints.iter().enumerate() {
                if interrupt.is_none() {
                    interrupt_id.cpu = Prid::from(cpu_num);
                    interrupt_id.interrupt_num = int_num as u8;
                    break 'outer;
                }
            }
        }

        self.inc_next_alloc_cpu();

        match self.get_int_entry_mut(interrupt_id) {
            Some(interrupt_emmiter) => Ok((interrupt_id, interrupt_emmiter.clone())),
            entry @ None => {
                let new_emmiter = Arc::new(
                    IMutex::new(BroadcastEventEmitter::new(allocator.clone())),
                    allocator.clone(),
                )?;

                *entry = Some(new_emmiter.clone());

                Ok((interrupt_id, new_emmiter))
            }
        }
    }

    fn remove_interrupt(&mut self, interrupt_id: InterruptId) {
        *self.get_int_entry_mut(interrupt_id) = None;
    }
}

/// A capability which lets userspace handle interrupts
#[derive(Debug)]
pub struct Interrupt {
    event_emmiter: Arc<InterruptEventEmmiter>,
    interrupt_id: InterruptId,
}

impl Interrupt {
    pub fn new(allocator: &HeapRef) -> KResult<Self> {
        let (interrupt_id, event_emmiter) = interrupt_manager().create_interrupt(allocator)?;
        Ok(Interrupt {
            event_emmiter,
            interrupt_id,
        })
    }

    pub fn interrupt_id(&self) -> InterruptId {
        self.interrupt_id
    }

    pub fn add_interrupt_listener(&self, listener: BroadcastEventListener) -> KResult<()> {
        self.event_emmiter.lock().add_listener(listener)
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        interrupt_manager().remove_interrupt(self.interrupt_id);
    }
}

impl CapObject for Interrupt {
    const TYPE: CapType = CapType::Interrupt;
}

/// A capability which allows userspace to allocate new interrupts
/// 
/// This capability will just call methods on the global interrupt manager
#[derive(Debug)]
pub struct IntAllocator;

impl CapObject for IntAllocator {
    const TYPE: CapType = CapType::IntAllocator;
}


static INTERRUPT_MANAGER: Once<IMutex<InterruptManager>> = Once::new();

pub fn interrupt_manager() -> IMutexGuard<'static, InterruptManager> {
    INTERRUPT_MANAGER.get().expect("interrupt manager not initialized").lock()
}

pub fn init_interrupt_manager(num_cpus: usize) -> KResult<()> {
    let manager = InterruptManager::new(root_alloc_ref(), num_cpus)?;
    INTERRUPT_MANAGER.call_once(|| IMutex::new(manager));
    Ok(())
}