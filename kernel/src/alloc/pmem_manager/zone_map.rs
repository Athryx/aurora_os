use crate::alloc::AllocRef;
use crate::container::{Vec, VecMap};
use crate::prelude::*;

// TODO: maybe use a better data structure than vec
// because some elements are removed from the middle, vec is not an optimal data structure,
// but it is the only one written at the moment, and this code is run once and is not performance critical

/// Used to initilize pmem allocator
/// Maps a certain size of memory to actual memory ranges
pub struct ZoneMap<T: VirtRange> {
    data: VecMap<usize, Vec<T>>,
    len: usize,
}

impl<T: VirtRange + core::fmt::Debug> ZoneMap<T> {
    pub fn new(allocator: AllocRef) -> Self {
        ZoneMap {
            data: VecMap::new(allocator),
            len: 0,
        }
    }

    pub fn insert(&mut self, zone: T) -> KResult<()> {
        match self.data.get_mut(&zone.size()) {
            Some(vec) => {
                vec.push(zone)?;
            },
            None => {
                let mut range_vec = Vec::new(self.data.alloc_ref());
                let zone_size = zone.size();
                range_vec.push(zone)?;
                self.data.insert(zone_size, range_vec)?;
            },
        }
        self.len += 1;
        Ok(())
    }

    pub fn remove_zone_at_least_size(&mut self, size: usize) -> Option<T> {
        let (_, gt_vec) = self.data.get_gt_mut(&size)?;
        let out = gt_vec.pop();
        if gt_vec.len() == 0 {
            self.data.remove_gt(&size);
        }
        out
    }

    pub fn remove_largest_zone(&mut self) -> Option<T> {
        let (_, max_vec) = self.data.get_max_mut()?;
        let out = max_vec.pop();
        if max_vec.len() == 0 {
            self.data.pop_max();
        }
        out
    }

    pub fn len(&self) -> usize {
        self.len
    }
}
