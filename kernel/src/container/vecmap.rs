use crate::prelude::*;

use crate::alloc::AllocRef;
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

pub struct VecMap<K: Ord, V>(Vec<MapNode<K, V>>);

impl<K: Ord, V> VecMap<K, V> {
	pub fn new(allocator: AllocRef) -> Self {
		VecMap(Vec::new(allocator))
	}

	pub fn try_with_capacity(allocator: AllocRef, cap: usize) -> KResult<Self> {
		Ok(VecMap(Vec::try_with_capacity(allocator, cap)?))
	}

	pub fn len(&self) -> usize {
		self.0.len()
	}

	pub fn cap(&self) -> usize {
		self.0.cap()
	}

	pub fn pop_max(&mut self) -> Option<(K, V)> {
		self.0.pop().map(|node| node.tuple())
	}
	
	pub fn pop_min(&mut self) -> Option<(K, V)> {
		if self.len() == 0 {
			None
		} else {
			Some(self.0.remove(0).tuple())
		}
	}

	// TODO: this return value might be ugly
	pub fn insert(&mut self, key: K, value: V) -> KResult<Option<V>> {
		let search_result = self.search(&key);
		let node = MapNode::new(key, value);
		match search_result {
			Ok(index) => {
				Ok(Some(self.0.replace(index, node).value))
			},
			Err(index) => {
				self.0.insert(index, node)?;
				Ok(None)
			},
		}
	}

	pub fn remove(&mut self, key: &K) -> Option<V> {
		match self.search(key) {
			Ok(index) => Some(self.0.remove(index).value),
			Err(_) => None
		}
	}

	// remove node greater than id if key does not exist
	// returns none if no node greater than ore equal to exists
	pub fn remove_gt(&mut self, key: &K) -> Option<(K, V)> {
		match self.search(key) {
			Ok(index) => Some(self.0.remove(index).tuple()),
			Err(index) => self.0.try_remove(index).map(|node| node.tuple()),
		}
	}

	// remove node less than than id if key does not exist
	// returns none if no node greater than ore equal to exists
	pub fn remove_lt(&mut self, key: &K) -> Option<(K, V)> {
		match self.search(key) {
			Ok(index) => Some(self.0.remove(index).tuple()),
			Err(index) => {
				if index == 0 {
					None
				} else {
					Some(self.0.remove(index - 1).tuple())
				}
			},
		}
	}

	// if key is contained in the map, Ok(index of element) is returned
	// else, Err(index where element should go) is returned
	fn search(&self, key: &K) -> Result<usize, usize>
	{
		self.0.binary_search_by(|node| {
			node.key.cmp(key)
		})
	}
}
