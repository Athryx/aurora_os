use spin::Once;
use array_init::array_init;
use sys::{CapType, EventData, InterruptTrigger};

use crate::mem::{root_alloc_ref, HeapRef};
use crate::gs_data::Prid;
use crate::sync::IMutexGuard;
use crate::event::{BroadcastEventEmitter, BroadcastEventListener};
use crate::prelude::*;
use crate::cap::CapObject;
use crate::sync::IMutex;
use super::{USER_INTERRUPT_COUNT, USER_INTERRUPT_START, INTERRUPT_COUNT};

type InterruptEventEmmiter = IMutex<BroadcastEventEmitter>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptId {
    pub cpu: Prid,
    pub interrupt_num: u8,
}

struct InterruptUseData {
    /// The cpu the next interrupt will be allocated on
    /// This is to try and spread interrupt handling out among cpus
    next_alloc_cpu: usize,
    /// The number of interrupts allocated for each interrupt vector
    interrupt_use_count: Vec<[usize; USER_INTERRUPT_COUNT]>,
}

impl InterruptUseData {
    fn iter_cpu_nums(&mut self) -> impl Iterator<Item = usize> {
        let first_range = self.next_alloc_cpu..self.interrupt_use_count.len();
        let second_range = 0..self.next_alloc_cpu;

        self.next_alloc_cpu += 1;
        if self.next_alloc_cpu == self.interrupt_use_count.len() {
            self.next_alloc_cpu = 0;
        }

        first_range.chain(second_range)
    }

    /// Incraments the use count for all interrupts in this region
    fn mark_region(&mut self, interrupt_id: InterruptId, count: usize) {
        let int_start = interrupt_id.interrupt_num as usize;

        for i in int_start..(int_start + count) {
            self.interrupt_use_count[interrupt_id.cpu.into()][i] += 1;
        }
    }

    /// Returns the number of interrupts currently in use in the give interrupt region
    fn interrupt_region_use_count(&self, base_interrupt_id: InterruptId, num_interrupts: usize) -> usize {
        let cpu_interrupts = &self.interrupt_use_count[base_interrupt_id.cpu.into()];

        let start_index = (base_interrupt_id.interrupt_num - USER_INTERRUPT_START) as usize;
        cpu_interrupts[start_index..(start_index + num_interrupts)]
            .iter()
            .sum()
    }

    /// Returns the base interrupt id to be used for `interrupt_count` interrupts
    fn find_interrupt_region(&mut self, interrupt_count: usize, interrupt_align: usize) -> KResult<InterruptId> {
        if interrupt_count == 0 || interrupt_count >= USER_INTERRUPT_COUNT {
            return Err(SysErr::InvlArgs);
        }

        let mut best_in_use_count = usize::MAX;
        let mut best_int_id = None;

        for cpu in self.iter_cpu_nums() {
            let mut prev_interrupt_end = USER_INTERRUPT_START as usize;

            loop {
                let int_start = align_up(prev_interrupt_end, interrupt_align);
                let int_end = int_start + interrupt_count;
                if int_end > INTERRUPT_COUNT {
                    break;
                }

                // count number of interrupts currently being used in this region
                let in_use_count = self.interrupt_use_count[cpu]
                    [(int_start - USER_INTERRUPT_START as usize)..(int_end - USER_INTERRUPT_START as usize)]
                    .iter()
                    .sum();

                // this is already the best in use count
                if in_use_count == 0 {
                    return Ok(InterruptId {
                        cpu: Prid::from(cpu),
                        interrupt_num: int_start as u8,
                    });
                } else if in_use_count < best_in_use_count {
                    best_in_use_count = in_use_count;
                    best_int_id = Some(InterruptId {
                        cpu: Prid::from(cpu),
                        interrupt_num: int_start as u8,
                    });
                }

                prev_interrupt_end = int_end;
            }
        }

        best_int_id.ok_or(SysErr::InvlArgs)
    }

    fn remove_interrupt(&mut self, interrupt_id: InterruptId) {
        self.interrupt_use_count[interrupt_id.cpu.into()][(interrupt_id.interrupt_num - USER_INTERRUPT_START) as usize] -= 1;
    }
}

/// The interrupt manager says where each userspace interrupt on a given cpu and interrupt vector,
/// which capability the interrupt event should be sent to
pub struct InterruptManager {
    use_data: IMutex<InterruptUseData>,
    interrupts: Vec<[IMutex<BroadcastEventEmitter>; USER_INTERRUPT_COUNT]>,
}

impl InterruptManager {
    fn new(allocator: HeapRef, num_cpus: usize) -> KResult<Self> {
        let mut interrupts = Vec::try_with_capacity(allocator.clone(), num_cpus)?;
        let mut interrupt_use_count = Vec::try_with_capacity(allocator, num_cpus)?;

        for _ in 0..num_cpus {
            // FIXME: array_init(...) page faults (null ptr dereference)
            interrupts.push(array_init(
                |_| IMutex::new(BroadcastEventEmitter::new(root_alloc_ref())),
            ))?;
            interrupt_use_count.push([0; USER_INTERRUPT_COUNT])?;
        }

        Ok(InterruptManager {
            use_data: IMutex::new(InterruptUseData {
                next_alloc_cpu: 0,
                interrupt_use_count,
            }),
            interrupts,
        })
    }

    fn get_interrupt_emitter(&self, interrupt_id: InterruptId) -> &IMutex<BroadcastEventEmitter> {
        &self.interrupts[interrupt_id.cpu.into()][(interrupt_id.interrupt_num - USER_INTERRUPT_START) as usize]
    }

    /// Triggers an interrupt event to be emmitted for the given interrupt
    pub fn notify_interrupt(&self, interrupt_id: InterruptId) -> KResult<()> {
        self.get_interrupt_emitter(interrupt_id)
            .lock()
            .emit_event(EventData::InterruptTrigger(InterruptTrigger))
    }

    /// Allocs a region interrupts with a certain alignmant and returns the InterruptId of the start of the interrupt region
    pub fn alloc_interrupts(&self, interrupt_count: usize, interrupt_align: usize) -> KResult<InterruptRegionIterator> {
        let mut use_data = self.use_data.lock();

        let int_id = use_data.find_interrupt_region(interrupt_count, interrupt_align)?;
        use_data.mark_region(int_id, interrupt_count);
        
        Ok(InterruptRegionIterator {
            base_id: int_id,
            count: interrupt_count,
            current_index: 0,
        })
    }

    fn remove_interrupt(&self, interrupt_id: InterruptId) {
        self.use_data.lock().remove_interrupt(interrupt_id);
    }
}

/// Owned iterator over a region of interrupts
///
/// Dropping this removes interrupts from interrupt manager
#[derive(Debug)]
pub struct InterruptRegionIterator {
    /// Interrupt id of first interrupt in region
    base_id: InterruptId,
    /// Number of interrupts in region
    count: usize,
    /// Current iteration index (starting from 0)
    current_index: usize,
}

impl InterruptRegionIterator {
    pub fn base_interrupt_id(&self) -> InterruptId {
        self.base_id
    }

    /// Gets the `InterruptId` corresponding to the given iteration index
    fn offset_interrupt_id(&self, index: usize) -> Option<InterruptId> {
        let index: u8 = index.try_into().ok()?;
        let interrupt_num = self.base_id.interrupt_num.checked_add(index)?;

        Some(InterruptId {
            cpu: self.base_id.cpu,
            interrupt_num,
        })
    }
}

impl Iterator for InterruptRegionIterator {
    type Item = Interrupt;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index == self.count {
            None
        } else {
            let interrupt_id = self.offset_interrupt_id(self.current_index)
                .expect("Itarating on InterruptRegionIterator with invalid region");
            self.current_index += 1;

            Some(interrupt_id.into())
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.count - self.current_index;

        (size, Some(size))
    }
}

impl ExactSizeIterator for InterruptRegionIterator {}

impl Drop for InterruptRegionIterator {
    fn drop(&mut self) {
        for i in self.current_index..self.count {
            let interrupt_id = self.offset_interrupt_id(i)
                .expect("Dropping InterruptRegionIterator with invalid region");
            interrupt_manager().remove_interrupt(interrupt_id);
        }
    }
}

/// A capability which lets userspace handle interrupts
#[derive(Debug)]
pub struct Interrupt {
    interrupt_id: InterruptId,
}

impl Interrupt {
    pub fn interrupt_id(&self) -> InterruptId {
        self.interrupt_id
    }

    pub fn add_interrupt_listener(&self, listener: BroadcastEventListener) -> KResult<()> {
        interrupt_manager()
            .get_interrupt_emitter(self.interrupt_id)
            .lock()
            .add_listener(listener)
    }
}

impl From<InterruptId> for Interrupt {
    fn from(interrupt_id: InterruptId) -> Self {
        Interrupt {
            interrupt_id,
        }
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