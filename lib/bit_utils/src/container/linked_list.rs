use core::fmt::{self, Debug, Formatter};
use core::ops::{Index, IndexMut};
use core::ptr::NonNull;
use core::marker::PhantomData;

use crate::MemOwner;

#[derive(Debug)]
pub struct ListNodeData<T> {
    prev: Option<NonNull<T>>,
    next: Option<NonNull<T>>,
}

impl<T> Default for ListNodeData<T> {
    fn default() -> Self {
        Self {
            prev: None,
            next: None,
        }
    }
}

unsafe impl<T> Send for ListNodeData<T> {}

pub trait ListNode: Sized {
    fn list_node_data(&self) -> &ListNodeData<Self>;
    fn list_node_data_mut(&mut self) -> &mut ListNodeData<Self>;

    fn prev(&self) -> Option<NonNull<Self>> {
        self.list_node_data().prev
    }

    fn next(&self) -> Option<NonNull<Self>> {
        self.list_node_data().next
    }

    fn prev_mut(&mut self) -> &mut Option<NonNull<Self>> {
        &mut self.list_node_data_mut().prev
    }

    fn next_mut(&mut self) -> &mut Option<NonNull<Self>> {
        &mut self.list_node_data_mut().next
    }

    fn addr(&self) -> usize {
        self as *const _ as *const () as usize
    }

    fn as_mut_ptr(&self) -> *mut Self {
        self as *const _ as *mut _
    }
}

/// An intrusive linked list which doesn't require allocation
pub struct LinkedList<T: ListNode> {
    start: Option<NonNull<T>>,
    end: Option<NonNull<T>>,
    len: usize,
}

impl<T: ListNode> LinkedList<T> {
    pub const fn new() -> Self {
        LinkedList {
            start: None,
            end: None,
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn push(&mut self, value: MemOwner<T>) -> &mut T {
        self.cursor_end_mut().insert_prev(value)
    }

    pub fn pop(&mut self) -> Option<MemOwner<T>> {
        self.cursor_end_mut().remove_prev()
    }

    pub fn push_front(&mut self, value: MemOwner<T>) -> &mut T {
        self.cursor_start_mut().insert_next(value)
    }

    pub fn pop_front(&mut self) -> Option<MemOwner<T>> {
        self.cursor_start_mut().remove_next()
    }

    pub fn insert(&mut self, index: usize, value: MemOwner<T>) -> Option<&mut T> {
        if index > self.len {
            None
        } else {
            Some(self.cursor_at_mut(index).insert_next(value))
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<MemOwner<T>> {
        if index >= self.len {
            None
        } else {
            self.cursor_at_mut(index).remove_next()
        }
    }

    /// Appends all elements from `other` linked list to this linked list
    pub fn append(&mut self, other: &mut LinkedList<T>) {
        if other.len() == 0 {
            return;
        }

        if self.len() == 0 {
            self.start = other.start;
            self.end = other.end;
            self.len = other.len;
        } else {
            unsafe {
                *self.end.unwrap().as_mut().next_mut() = other.start;
                *other.start.unwrap().as_mut().prev_mut() = self.end;
            }

            self.end = other.end;

            self.len += other.len;
        }

        *other = LinkedList::new();
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        unsafe {
            Some(self.get_node(index)?.as_ref())
        }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        unsafe {
            Some(self.get_node(index)?.as_mut())
        }
    }

    pub fn cursor_start(&self) -> Cursor<T> {
        self.cursor_at(0)
    }

    pub fn cursur_end(&self) -> Cursor<T> {
        self.cursor_at(self.len)
    }

    pub fn cursor_at(&self, index: usize) -> Cursor<T> {
        assert!(index <= self.len, "invalid cursor index");

        if index == 0 {
            Cursor {
                prev: None,
                next: self.get_node(0),
                _marker: PhantomData,
            }
        } else {
            let node = self.get_node(index - 1).unwrap();

            Cursor {
                prev: Some(node),
                next: unsafe { node.as_ref().next() },
                _marker: PhantomData,
            }
        }
    }

    pub fn cursor_start_mut(&mut self) -> CursorMut<T> {
        self.cursor_at_mut(0)
    }

    pub fn cursor_end_mut(&mut self) -> CursorMut<T> {
        self.cursor_at_mut(self.len)
    }

    pub fn cursor_at_mut(&mut self, index: usize) -> CursorMut<T> {
        assert!(index <= self.len, "invalid cursor index");

        if index == 0 {
            CursorMut {
                prev: None,
                next: self.get_node(0),
                list: self,
            }
        } else {
            let node = self.get_node(index - 1).unwrap();

            CursorMut {
                prev: Some(node),
                next: unsafe { node.as_ref().next() },
                list: self,
            }
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            start: self.start,
            end: self.end,
            len: self.len,
            marker: PhantomData,
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            start: self.start,
            end: self.end,
            len: self.len,
            marker: PhantomData,
        }
    }

    fn get_node(&self, index: usize) -> Option<NonNull<T>> {
        if index >= self.len {
            return None;
        }

        let mut node;
        if index * 2 > self.len {
            node = self.end.unwrap();

            for _ in 0..(self.len - index - 1) {
                unsafe {
                    node = node.as_ref().prev().unwrap();
                }
            }
        } else {
            node = self.start.unwrap();

            for _ in 0..index {
                unsafe {
                    node = node.as_ref().next().unwrap();
                }
            }
        }

        Some(node)
    }
}

/// A cursor points between two nodes in a linked list
pub struct Cursor<'a, T: ListNode> {
    prev: Option<NonNull<T>>,
    next: Option<NonNull<T>>,
    _marker: PhantomData<&'a LinkedList<T>>,
}

impl<'a, T: ListNode> Cursor<'a, T> {
    pub fn prev(&self) -> Option<&'a T> {
        unsafe {
            Some(self.prev?.as_ref())
        }
    }

    pub fn next(&self) -> Option<&'a T> {
        unsafe {
            Some(self.next?.as_ref())
        }
    }

    /// Advances the cursor over the previous element if it exists
    /// 
    /// Returns the previous element it advanced over, or none if it didn't advance
    pub fn move_prev(&mut self) -> Option<&'a T> {
        if let Some(prev) = self.prev() {
            self.next = self.prev;
            self.prev = prev.prev();
            Some(prev)
        } else {
            None
        }
    }

    /// Advances the cursor over the next element if it exists
    /// 
    /// Returns the next element it advanced over, or none if it didn't advance
    pub fn move_next(&mut self) -> Option<&'a T> {
        if let Some(next) = self.next() {
            self.prev = self.next;
            self.next = next.next();
            Some(next)
        } else {
            None
        }
    }
}

/// A cursor points between two nodes in a linked list, also allows mutation
pub struct CursorMut<'a, T: ListNode> {
    prev: Option<NonNull<T>>,
    next: Option<NonNull<T>>,
    list: &'a mut LinkedList<T>,
}

impl<'a, T: ListNode> CursorMut<'a, T> {
    pub fn prev(&self) -> Option<&'a T> {
        unsafe {
            Some(self.prev?.as_ref())
        }
    }

    pub fn prev_mut(&self) -> Option<&'a mut T> {
        unsafe {
            Some(self.prev?.as_mut())
        }
    }

    pub fn next(&self) -> Option<&'a T> {
        unsafe {
            Some(self.next?.as_ref())
        }
    }

    pub fn next_mut(&self) -> Option<&'a mut T> {
        unsafe {
            Some(self.next?.as_mut())
        }
    }

    /// Advances the cursor over the previous element if it exists
    /// 
    /// Returns the previous element it advanced over, or none if it didn't advance
    pub fn move_prev(&mut self) -> Option<&'a mut T> {
        if let Some(prev) = self.prev_mut() {
            self.next = self.prev;
            self.prev = prev.prev();
            Some(prev)
        } else {
            None
        }
    }

    /// Advances the cursor over the next element if it exists
    /// 
    /// Returns the next element it advanced over, or none if it didn't advance
    pub fn move_next(&mut self) -> Option<&'a mut T> {
        if let Some(next) = self.next_mut() {
            self.prev = self.next;
            self.next = next.next();
            Some(next)
        } else {
            None
        }
    }

    /// Inserts the value into the list and sets all the prev and next pointers and the list size
    fn insert_inner(&mut self, value: &mut T) {
        *value.prev_mut() = self.prev;
        *value.next_mut() = self.next;

        let ptr = Some(NonNull::new(value).unwrap());

        if let Some(prev) = self.prev_mut() {
            *prev.next_mut() = ptr;
        } else {
            self.list.start = ptr;
        }

        if let Some(next) = self.next_mut() {
            *next.prev_mut() = ptr;
        } else {
            self.list.end = ptr;
        }

        self.list.len += 1;
    }

    /// Inserts `value` before the specified cursor
    pub fn insert_prev(&mut self, mut value: MemOwner<T>) -> &'a mut T {
        self.insert_inner(&mut value);
        self.prev = Some(NonNull::new(&mut *value).unwrap());

        value.leak()
    }

    /// Inserts `value` after the specified cursor
    pub fn insert_next(&mut self, mut value: MemOwner<T>) -> &'a mut T {
        self.insert_inner(&mut value);
        self.next = Some(NonNull::new(&mut *value).unwrap());

        value.leak()
    }

    /// Removes the value from the list and sets the prev and nex pointers and the list size
    fn remove_inner(&mut self, value: &mut T) {
        if let Some(mut prev) = value.prev() {
            unsafe {
                *(prev.as_mut()).next_mut() = value.next();
            }
        } else {
            self.list.start = value.next();
        }

        if let Some(mut next) = value.next() {
            unsafe {
                *(next.as_mut()).prev_mut() = value.prev();
            }
        } else {
            self.list.end = value.prev();
        }

        self.list.len -= 1;
    }

    /// Removes the prevoius value if it exists, or returns None
    pub fn remove_prev(&mut self) -> Option<MemOwner<T>> {
        let mut node = unsafe { MemOwner::from_raw(self.prev?.as_ptr()) };
        self.prev = node.prev();

        self.remove_inner(&mut node);

        Some(node)
    }

    /// Removes the prevoius value if it exists, or returns None
    pub fn remove_next(&mut self) -> Option<MemOwner<T>> {
        let mut node = unsafe { MemOwner::from_raw(self.next?.as_ptr()) };
        self.next = node.next();

        self.remove_inner(&mut node);

        Some(node)
    }
}

impl<T: ListNode> Index<usize> for LinkedList<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

impl<T: ListNode> IndexMut<usize> for LinkedList<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("index out of bounds")
    }
}

impl<'a, T: ListNode> IntoIterator for &'a LinkedList<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: ListNode> IntoIterator for &'a mut LinkedList<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T: ListNode + Debug> Debug for LinkedList<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self).finish().unwrap();
        Ok(())
    }
}

impl<T: ListNode> Default for LinkedList<T> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<T: ListNode + Send> Send for LinkedList<T> {}

// NOTE: it is safe to deallocate nodes returned from Iter and IterMut
pub struct Iter<'a, T: ListNode> {
    start: Option<NonNull<T>>,
    end: Option<NonNull<T>>,
    len: usize,
    marker: PhantomData<&'a T>,
}

impl<'a, T: ListNode> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            let out = unsafe { self.start?.as_ref() };
            self.start = out.next();
            self.len -= 1;
            Some(out)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl<'a, T: ListNode> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            let out = unsafe { self.end?.as_ref() };
            self.end = out.prev();
            self.len -= 1;
            Some(out)
        }
    }
}

impl<T: ListNode> ExactSizeIterator for Iter<'_, T> {}
impl<T: ListNode> core::iter::FusedIterator for Iter<'_, T> {}

pub struct IterMut<'a, T: ListNode> {
    start: Option<NonNull<T>>,
    end: Option<NonNull<T>>,
    len: usize,
    marker: PhantomData<&'a T>,
}

impl<'a, T: ListNode> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            let out = unsafe { self.start?.as_mut() };
            self.start = out.next();
            self.len -= 1;
            Some(out)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl<'a, T: ListNode> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            let out = unsafe { self.end?.as_mut() };
            self.end = out.prev();
            self.len -= 1;
            Some(out)
        }
    }
}

impl<T: ListNode> ExactSizeIterator for IterMut<'_, T> {}
impl<T: ListNode> core::iter::FusedIterator for IterMut<'_, T> {}
