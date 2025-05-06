use core::hash::{Hash, Hasher};
use core::iter::FusedIterator;
use core::ops::{Index, IndexMut};
use core::slice;
use core::fmt;

use siphasher::sip::SipHasher;

use super::{vec, Vec};
use crate::alloc::HeapRef;
use crate::prelude::*;

enum HashMapCell<K, V> {
    Empty,
    Occupied(K, V),
    Deleted,
}

impl<K, V> HashMapCell<K, V> {
    fn is_free(&self) -> bool {
        matches!(self, Self::Empty | Self::Deleted)
    }
}

/// Basic hashmap implementation which uses siphash 2-4 and linear open addressing
pub struct HashMap<K: Hash + Eq, V> {
    data: Vec<HashMapCell<K, V>>,
    len: usize,
}

impl<K: Hash + Eq, V> HashMap<K, V> {
    pub fn new(allocer: HeapRef) -> Self {
        HashMap {
            data: Vec::new(allocer),
            len: 0,
        }
    }

    pub fn try_with_capacity(allocer: HeapRef, capacity: usize) -> KResult<Self> {
        let mut out = HashMap {
            data: Vec::try_with_capacity(allocer, capacity)?,
            len: 0,
        };

        for _ in 0..capacity {
            out.data.push(HashMapCell::Empty)?;
        }

        Ok(out)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    pub fn clear(&mut self) {
        self.data.clear()
    }

    pub fn iter(&self) -> Iter<K, V> {
        Iter(self.data.iter())
    }

    pub fn into_iter(self) -> IntoIter<K, V> {
        IntoIter(self.data.into_iter())
    }

    /// Rehashes the hash table if it is past a certain capacity thresh hold
    fn try_rehash(&mut self) -> KResult<()> {
        // load factor of 0.75
        if 4 * self.len > 3 * self.data.len() {
            let new_map = Self::try_with_capacity(self.data.alloc_ref(), 2 * self.data.len())?;
            let old_map = core::mem::replace(self, new_map);

            for (key, value) in old_map.into_iter() {
                self.insert(key, value)?;
            }
        } else if self.data.len() == 0 {
            // make a size 1 map otherwise things break
            let new_map = Self::try_with_capacity(self.data.alloc_ref(), 1)?;
            let _ = core::mem::replace(self, new_map);
        }

        Ok(())
    }

    fn get_key_start_index(&self, key: &K) -> usize {
        // TODO: use a hash builder with keys for siphash to prevent potential denial of service attacks
        let mut hasher = SipHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.data.len()
    }

    /// Returns the old value if it exists
    pub fn insert(&mut self, key: K, value: V) -> KResult<Option<V>> {
        self.try_rehash()?;
        let mut i = self.get_key_start_index(&key);
        loop {
            if self.data[i].is_free() {
                self.data[i] = HashMapCell::Occupied(key, value);
                self.len += 1;
                return Ok(None);
            }
            if let HashMapCell::Occupied(ref old_key, _) = self.data[i] && old_key == &key {
                // this should always match, its just because using a normal let will not allow a fallible pattern
                if let HashMapCell::Occupied(_, old_value) = core::mem::replace(&mut self.data[i], HashMapCell::Occupied(key, value)) {
                    return Ok(Some(old_value));
                } else {
                    unreachable!();
                }
            }

            i = (i + 1) % self.data.len();
        }
    }

    // gets the index in the data array
    fn get_index_of_key(&self, key: &K) -> Option<usize> {
        // prevent divide by 0 in get_key_start_index
        if self.data.len() == 0 {
            return None;
        }

        let mut i = self.get_key_start_index(key);
        let start_i = i;
        loop {
            if matches!(self.data[i], HashMapCell::Empty) {
                return None;
            }
            if let HashMapCell::Occupied(ref current_key, _) = self.data[i] && current_key == key {
                return Some(i);
            }
            i = (i + 1) % self.data.len();

            // at lower sizes hashmap will be completely full, so this avoids infinite loop
            if i == start_i {
                return None;
            }
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let i = self.get_index_of_key(key)?;
        if let HashMapCell::Occupied(_, value) = core::mem::replace(&mut self.data[i], HashMapCell::Deleted) {
            Some(value)
        } else {
            None
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        if let HashMapCell::Occupied(_, ref value) = self.data[self.get_index_of_key(key)?] {
            Some(value)
        } else {
            unreachable!();
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        // do this to make borrow checker happy
        let index = self.get_index_of_key(key)?;
        if let HashMapCell::Occupied(_, ref mut value) = self.data[index] {
            Some(value)
        } else {
            unreachable!();
        }
    }
}

impl<K: Eq + Hash + fmt::Debug, V: fmt::Debug> fmt::Debug for HashMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: Eq + Hash, V> Index<&K> for HashMap<K, V> {
    type Output = V;

    fn index(&self, index: &K) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

impl<K: Eq + Hash, V> IndexMut<&K> for HashMap<K, V> {
    fn index_mut(&mut self, index: &K) -> &mut Self::Output {
        self.get_mut(index).expect("index out of bounds")
    }
}

pub struct Iter<'a, K: Hash + Eq, V>(slice::Iter<'a, HashMapCell<K, V>>);

impl<'a, K: Hash + Eq, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        for data in self.0.by_ref() {
            if let HashMapCell::Occupied(key, value) = data {
                return Some((key, value));
            }
        }
        None
    }
}

impl<K: Hash + Eq, V> FusedIterator for Iter<'_, K, V> {}

pub struct IterMut<'a, K: Hash + Eq, V>(slice::IterMut<'a, HashMapCell<K, V>>);

impl<'a, K: Hash + Eq, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        for data in self.0.by_ref() {
            if let HashMapCell::Occupied(key, value) = data {
                return Some((key, value));
            }
        }
        None
    }
}

impl<K: Hash + Eq, V> FusedIterator for IterMut<'_, K, V> {}

pub struct IntoIter<K: Hash + Eq, V>(vec::IntoIter<HashMapCell<K, V>>);

impl<K: Hash + Eq, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        for data in self.0.by_ref() {
            if let HashMapCell::Occupied(key, value) = data {
                return Some((key, value));
            }
        }
        None
    }
}

impl<K: Hash + Eq, V> FusedIterator for IntoIter<K, V> {}
