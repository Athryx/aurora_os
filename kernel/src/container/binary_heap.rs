use core::mem;

use crate::prelude::*;
use crate::mem::HeapRef;

/// A max heap binary heap
#[derive(Debug)]
pub struct BinaryHeap<T: Ord> {
    data: Vec<T>,
}

impl<T: Ord> BinaryHeap<T> {
    pub fn new(allocator: HeapRef) -> Self {
        BinaryHeap {
            data: Vec::new(allocator),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    fn parent(index: usize) -> Option<usize> {
        if index == 0 {
            None
        } else {
            Some((index - 1) / 2)
        }
    }

    fn left_child(&self, index: usize) -> Option<usize> {
        let left_child = (index * 2) + 1;
        if left_child >= self.len() {
            None
        } else {
            Some(left_child)
        }
    }

    fn right_child(&self, index: usize) -> Option<usize> {
        let right_child = (index * 2) + 2;
        if right_child >= self.len() {
            None
        } else {
            Some(right_child)
        }
    }

    fn largest_child(&self, index: usize) -> Option<usize> {
        match (self.left_child(index), self.right_child(index)) {
            (Some(left), Some(right)) => {
                if self.data[left] > self.data[right] {
                    Some(left)
                } else {
                    Some(right)
                }
            },
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        }
    }

    pub fn push(&mut self, object: T) -> KResult<()> {
        self.data.push(object)?;

        let mut index = self.len() - 1;

        while let Some(parent) = Self::parent(index) {
            if self.data[parent] < self.data[index] {
                self.data.swap(parent, index);
                index = parent;
            } else {
                break;
            }
        }

        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        let tail = self.data.pop()?;
        if self.len() == 0 {
            return Some(tail);
        }

        let out = mem::replace(&mut self.data[0], tail);
        let mut index = 0;

        while let Some(largest_child) = self.largest_child(index) {
            if self.data[largest_child] > self.data[index] {
                self.data.swap(index, largest_child);
                index = largest_child;
            } else {
                break;
            }
        }

        Some(out)
    }

    pub fn peek(&self) -> Option<&T> {
        self.data.get(0)
    }

    pub fn peek_mut(&mut self) -> Option<&mut T> {
        self.data.get_mut(0)
    }
}