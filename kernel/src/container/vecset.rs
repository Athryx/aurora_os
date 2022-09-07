use core::cmp::Ordering;

use crate::prelude::*;
use crate::alloc::{AllocRef, HeapAllocator};
use super::VecMap;

pub struct VecSet<T: Ord>(VecMap<T, ()>);

impl<T: Ord> VecSet<T> {
	pub fn new(allocator: AllocRef) -> Self {
		VecSet(VecMap::new(allocator))
	}

	pub fn with_compare(allocator: AllocRef, compare: fn(&T, &T) -> Ordering) -> Self {
		VecSet(VecMap::with_compare(allocator, compare))
	}

	pub fn try_with_capacity(allocator: AllocRef, cap: usize) -> KResult<Self> {
		Ok(VecSet(VecMap::try_with_capacity(allocator, cap)?))
	}

	pub fn try_with_capacity_compare(allocator: AllocRef, cap: usize, compare: fn(&T, &T) -> Ordering) -> KResult<Self> {
		Ok(VecSet(VecMap::try_with_capacity_compare(allocator, cap, compare)?))
	}

	pub fn len(&self) -> usize {
		self.0.len()
	}

	pub fn cap(&self) -> usize {
		self.0.cap()
	}

	pub fn allocator(&mut self) -> &dyn HeapAllocator {
		self.0.allocator()
	}

	pub fn pop_max(&mut self) -> Option<T> {
		self.0.pop_max().map(|v| v.0)
	}
	
	pub fn pop_min(&mut self) -> Option<T> {
		self.0.pop_min().map(|v| v.0)
	}

	// returns true if it vecset did not previously have key, returns true if it does and updates the old key
	pub fn insert(&mut self, object: T) -> KResult<bool> {
		self.0.insert(object, ()).map(|v| v.is_some())
	}

	// returns true if value was present in set
	pub fn remove(&mut self, object: &T) -> bool {
		self.0.remove(object).is_some()
	}

	// remove node greater than id if key does not exist
	// returns none if no node greater than ore equal to exists
	pub fn remove_gt(&mut self, object: &T) -> Option<T> {
		self.0.remove_gt(object).map(|v| v.0)
	}

	// remove node greater than id if key does not exist
	// returns none if no node greater than ore equal to exists
	pub fn remove_lt(&mut self, object: &T) -> Option<T> {
		self.0.remove_lt(object).map(|v| v.0)
	}
}
