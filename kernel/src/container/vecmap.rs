use core::cmp::Ordering;
use core::ops::{RangeBounds, Bound};
use core::slice::Iter;
use core::iter::FusedIterator;

use crate::prelude::*;
use crate::alloc::{AllocRef, HeapAllocator};
use super::Vec;

#[derive(Debug)]
struct MapNode<K: Ord, V> {
	key: K,
	value: V,
}

impl<K: Ord, V> MapNode<K, V>
{
	fn new(key: K, value: V) -> Self
	{
		MapNode {
			key,
			value,
		}
	}

	fn tuple(self) -> (K, V) {
		(self.key, self.value)
	}
}

pub struct VecMap<K: Ord, V> {
	data: Vec<MapNode<K, V>>,
	compare: Option<fn(&K, &K) -> Ordering>,
}

impl<K: Ord, V> VecMap<K, V> {
	pub fn new(allocator: AllocRef) -> Self {
		VecMap {
			data: Vec::new(allocator),
			compare: None
		}
	}

	// compare returns wether first argument is less than, equal to, or greater than second argument
	pub fn with_compare(allocator: AllocRef, compare: fn(&K, &K) -> Ordering) -> Self {
		VecMap {
			data: Vec::new(allocator),
			compare: Some(compare),
		}
	}

	pub fn try_with_capacity(allocator: AllocRef, cap: usize) -> KResult<Self> {
		Ok(VecMap {
			data: Vec::try_with_capacity(allocator, cap)?,
			compare: None,
		})
	}

	pub fn try_with_capacity_compare(allocator: AllocRef, cap: usize, compare: fn(&K, &K) -> Ordering) -> KResult<Self> {
		Ok(VecMap {
			data: Vec::try_with_capacity(allocator, cap)?,
			compare: Some(compare),
		})
	}

	pub fn len(&self) -> usize {
		self.data.len()
	}

	pub fn cap(&self) -> usize {
		self.data.cap()
	}

	pub fn allocator(&self) -> &dyn HeapAllocator {
		self.data.allocator()
	}

	pub fn get(&self, key: &K) -> Option<&V> {
		self.search(key).ok().map(|index| &self.data[index].value)
	}

	pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
		self.search(key).ok().map(|index| &mut self.data[index].value)
	}

	pub fn pop_max(&mut self) -> Option<(K, V)> {
		self.data.pop().map(|node| node.tuple())
	}

	pub fn pop_min(&mut self) -> Option<(K, V)> {
		if self.len() == 0 {
			None
		} else {
			Some(self.data.remove(0).tuple())
		}
	}

	// TODO: this return value might be ugly
	pub fn insert(&mut self, key: K, value: V) -> KResult<Option<V>> {
		let search_result = self.search(&key);
		let node = MapNode::new(key, value);
		match search_result {
			Ok(index) => {
				Ok(Some(self.data.replace(index, node).value))
			},
			Err(index) => {
				self.data.insert(index, node)?;
				Ok(None)
			},
		}
	}

	pub fn remove(&mut self, key: &K) -> Option<V> {
		match self.search(key) {
			Ok(index) => Some(self.data.remove(index).value),
			Err(_) => None
		}
	}

	// remove node greater than id if key does not exist
	// returns none if no node greater than ore equal to exists
	pub fn remove_gt(&mut self, key: &K) -> Option<(K, V)> {
		match self.search(key) {
			Ok(index) => Some(self.data.remove(index).tuple()),
			Err(index) => self.data.try_remove(index).map(|node| node.tuple()),
		}
	}

	// remove node less than than id if key does not exist
	// returns none if no node greater than ore equal to exists
	pub fn remove_lt(&mut self, key: &K) -> Option<(K, V)> {
		match self.search(key) {
			Ok(index) => Some(self.data.remove(index).tuple()),
			Err(index) => {
				if index == 0 {
					None
				} else {
					Some(self.data.remove(index - 1).tuple())
				}
			},
		}
	}

	// panics if start > end, or start equals end and both are excluded
	pub fn range<R: RangeBounds<K>>(&self, range: R) -> RangeIter<'_, K, V> {
		let start_bound = range.start_bound();
		let end_bound = range.end_bound();

		match start_bound {
			Bound::Included(key) => match end_bound {
				Bound::Included(key2) | Bound::Excluded(key2) => assert!(key <= key2, "invalid range"),
				_ => (),
			},
			Bound::Excluded(key) => match end_bound {
				Bound::Included(key2) => assert!(key <= key2, "invalid range"),
				Bound::Excluded(key2) => assert!(key < key2, "invalid range"),
				_ => (),
			},
			_ => (),
		}

		let start = range.start_bound().map(|key| {
			match self.search(&key) {
				Ok(index) => index,
				Err(index) => index + 1,
			}
		});

		let end = range.start_bound().map(|key| {
			match self.search(&key) {
				Ok(index) => index,
				Err(index) => index - 1,
			}
		});

		// panic safety: start and end should already be in the vec
		let slice_iter = self.data[(start, end)].iter();
		RangeIter {
			inner: slice_iter,
		}
	}

	// if key is contained in the map, Ok(index of element) is returned
	// else, Err(index where element should go) is returned
	fn search(&self, key: &K) -> Result<usize, usize>
	{
		self.data.binary_search_by(|node| {
			if let Some(compare) = self.compare {
				compare(&node.key, key)
			} else {
				node.key.cmp(key)
			}
		})
	}
}

pub struct RangeIter<'a, K: Ord, V> {
	inner: Iter<'a, MapNode<K, V>>,
}

impl<'a, K: Ord, V> Iterator for RangeIter<'a, K, V> {
	type Item = (&'a K, &'a V);

	fn next(&mut self) -> Option<Self::Item> {
		self.inner.next().map(|node| (&node.key, &node.value))
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.inner.size_hint()
	}
}

impl<K: Ord, V> DoubleEndedIterator for RangeIter<'_, K, V> {
	fn next_back(&mut self) -> Option<Self::Item> {
		self.inner.next_back().map(|node| (&node.key, &node.value))
	}
}

impl<K: Ord, V> ExactSizeIterator for RangeIter<'_, K, V> {}
impl<K: Ord, V> FusedIterator for RangeIter<'_, K, V> {}
