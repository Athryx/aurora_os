use core::cmp::Ordering;
use core::iter::FusedIterator;
use core::ops::{Bound, RangeBounds};
use core::slice::{Iter, IterMut};
use core::fmt::{self, Debug};

use super::Vec;
use crate::mem::HeapRef;
use crate::prelude::*;

#[derive(Debug)]
struct MapNode<K: Ord, V> {
    key: K,
    value: V,
}

impl<K: Ord, V> MapNode<K, V> {
    fn new(key: K, value: V) -> Self {
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
    pub fn new(allocator: HeapRef) -> Self {
        VecMap {
            data: Vec::new(allocator),
            compare: None,
        }
    }

    // compare returns wether first argument is less than, equal to, or greater than second argument
    pub fn with_compare(allocator: HeapRef, compare: fn(&K, &K) -> Ordering) -> Self {
        VecMap {
            data: Vec::new(allocator),
            compare: Some(compare),
        }
    }

    pub fn try_with_capacity(allocator: HeapRef, cap: usize) -> KResult<Self> {
        Ok(VecMap {
            data: Vec::try_with_capacity(allocator, cap)?,
            compare: None,
        })
    }

    pub fn try_with_capacity_compare(
        allocator: HeapRef,
        cap: usize,
        compare: fn(&K, &K) -> Ordering,
    ) -> KResult<Self> {
        Ok(VecMap {
            data: Vec::try_with_capacity(allocator, cap)?,
            compare: Some(compare),
        })
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn cap(&self) -> usize {
        self.data.capacity()
    }

    pub fn allocator(&mut self) -> &mut HeapRef {
        self.data.allocator()
    }

    pub fn alloc_ref(&self) -> HeapRef {
        self.data.alloc_ref()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.search(key).ok().map(|index| &self.data[index].value)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.search(key).ok().map(|index| &mut self.data[index].value)
    }

    pub fn get_max_mut(&mut self) -> Option<(&K, &mut V)> {
        let len = self.len();
        if len == 0 {
            None
        } else {
            let node = &mut self.data[len - 1];
            Some((&node.key, &mut node.value))
        }
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
            Ok(index) => Ok(Some(self.data.replace(index, node).value)),
            Err(index) => {
                self.data.insert(index, node)?;
                Ok(None)
            },
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        match self.search(key) {
            Ok(index) => Some(self.data.remove(index).value),
            Err(_) => None,
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

    pub fn get_gt(&self, key: &K) -> Option<(&K, &V)> {
        self.range(key..).next()
    }

    pub fn get_lt(&self, key: &K) -> Option<(&K, &V)> {
        self.range(..key).next_back()
    }

    pub fn get_gt_mut(&mut self, key: &K) -> Option<(&K, &mut V)> {
        self.range_mut(key..).next()
    }

    pub fn get_lt_mut(&mut self, key: &K) -> Option<(&K, &mut V)> {
        self.range_mut(..key).next_back()
    }

    // used for range and range mute methods
    // given a range of keys, will return a start and end bound for the indexes in the vecmap
    // that these keys start and end at
    fn get_range_iter_bounds<R: RangeBounds<K>>(&self, range: R) -> (Bound<usize>, Bound<usize>) {
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

        let start = match range.start_bound() {
            Bound::Included(key) => match self.search(key) {
                Ok(index) => Bound::Included(index),
                Err(index) => Bound::Included(index),
            },
            Bound::Excluded(key) => match self.search(key) {
                Ok(index) => Bound::Excluded(index),
                Err(index) => Bound::Included(index),
            },
            Bound::Unbounded => Bound::Unbounded,
        };

        let end = match range.end_bound() {
            Bound::Included(key) => match self.search(key) {
                Ok(index) => Bound::Included(index),
                Err(index) => Bound::Excluded(index),
            },
            Bound::Excluded(key) => match self.search(key) {
                Ok(index) => Bound::Excluded(index),
                Err(index) => Bound::Excluded(index),
            },
            Bound::Unbounded => Bound::Unbounded,
        };

        (start, end)
    }

    // panics if start > end, or start equals end and both are excluded
    pub fn range<R: RangeBounds<K>>(&self, range: R) -> RangeIter<'_, K, V> {
        let (start, end) = self.get_range_iter_bounds(range);

        // panic safety: start and end should already be in the vec
        let slice_iter = self.data[(start, end)].iter();
        RangeIter {
            inner: slice_iter,
        }
    }

    // panics if start > end, or start equals end and both are excluded
    pub fn range_mut<R: RangeBounds<K>>(&mut self, range: R) -> RangeIterMut<'_, K, V> {
        let (start, end) = self.get_range_iter_bounds(range);

        // panic safety: start and end should already be in the vec
        let slice_iter = self.data[(start, end)].iter_mut();
        RangeIterMut {
            inner: slice_iter,
        }
    }

    // if key is contained in the map, Ok(index of element) is returned
    // else, Err(index where element should go) is returned
    fn search(&self, key: &K) -> Result<usize, usize> {
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

pub struct RangeIterMut<'a, K: Ord, V> {
    inner: IterMut<'a, MapNode<K, V>>,
}

impl<'a, K: Ord, V> Iterator for RangeIterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|node| (&node.key, &mut node.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K: Ord, V> DoubleEndedIterator for RangeIterMut<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|node| (&node.key, &mut node.value))
    }
}

impl<K: Ord, V> ExactSizeIterator for RangeIterMut<'_, K, V> {}
impl<K: Ord, V> FusedIterator for RangeIterMut<'_, K, V> {}

impl<K: Ord + Debug, V: Debug> Debug for VecMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.range(..)).finish()
    }
}