use core::cell::Cell;
//use core::ops::{Index, IndexMut};
use core::fmt::{self, Debug, Formatter};
use core::ops::{Index, IndexMut};
use core::ptr::Thin;

use crate::prelude::*;
use crate::mem::MemOwner;

#[derive(Debug, Clone)]
pub struct ListNodeData<T: ?Sized + Thin> {
	prev: Cell<*mut T>,
	next: Cell<*mut T>,
}

impl<T: ?Sized + Thin> Default for ListNodeData<T> {
	fn default() -> Self {
		Self {
			prev: Cell::new(null_mut()),
			next: Cell::new(null_mut()),
		}
	}
}

unsafe impl<T: ?Sized + Thin> Send for ListNodeData<T> {}

pub trait ListNode: Thin {
	fn list_node_data(&self) -> &ListNodeData<Self>;

	fn prev_ptr(&self) -> *mut Self {
		self.list_node_data().prev.get()
	}

	fn next_ptr(&self) -> *mut Self {
		self.list_node_data().next.get()
	}

	fn set_prev(&self, prev: *mut Self) {
		self.list_node_data().prev.set(prev)
	}

	fn set_next(&self, next: *mut Self) {
		self.list_node_data().next.set(next)
	}

	fn addr(&self) -> usize {
		self as *const _ as *const () as usize
	}

	fn as_mut_ptr(&self) -> *mut Self {
		self as *const _ as *mut _
	}
}

// TODO: maybe make in into_iter method
// this linked list doesn't require memory allocation
pub struct LinkedList<T: ListNode>
{
	start: *mut T,
	end: *mut T,
	len: usize,
}

impl<T: ListNode> LinkedList<T>
{
	pub const fn new() -> Self
	{
		LinkedList {
			start: null_mut(),
			end: null_mut(),
			len: 0,
		}
	}

	pub fn len(&self) -> usize
	{
		self.len
	}

	// NOTE: first node prev and last store null
	pub fn push(&mut self, val: MemOwner<T>) -> &mut T
	{
		if self.len == 0 {
			self.start = val.ptr_mut();
			val.set_prev(null_mut());
			val.set_next(null_mut());
		} else {
			unsafe {
				self.end.as_ref().unwrap().set_next(val.ptr_mut());
			}
			val.set_prev(self.end);
			val.set_next(null_mut());
		}
		self.end = val.ptr_mut();
		self.len += 1;

		val.leak()
	}

	pub fn pop(&mut self) -> Option<MemOwner<T>>
	{
		if self.len == 0 {
			return None;
		}

		let out;
		unsafe {
			out = MemOwner::from_raw(self.end);
			let out_ref = self.end.as_ref().unwrap();
			if self.len > 1 {
				self.end = out_ref.prev_ptr();
				self.end.as_ref().unwrap().set_next(null_mut());
			}
		}

		self.len -= 1;
		Some(out)
	}

	pub fn push_front(&mut self, val: MemOwner<T>) -> &mut T
	{
		if self.len == 0 {
			self.end = val.ptr_mut();
			val.set_prev(null_mut());
			val.set_next(null_mut());
		} else {
			unsafe {
				self.start.as_ref().unwrap().set_prev(val.ptr_mut());
			}
			val.set_next(self.start);
			val.set_prev(null_mut());
		}
		self.start = val.ptr_mut();
		self.len += 1;

		val.leak()
	}

	pub fn pop_front(&mut self) -> Option<MemOwner<T>>
	{
		if self.len == 0 {
			return None;
		}

		let out;
		unsafe {
			out = MemOwner::from_raw(self.start);
			let out_ref = self.start.as_ref().unwrap();
			if self.len > 1 {
				self.start = out_ref.next_ptr();
				self.start.as_ref().unwrap().set_prev(null_mut());
			}
		}

		self.len -= 1;
		Some(out)
	}

	pub fn insert(&mut self, index: usize, val: MemOwner<T>) -> Option<&mut T>
	{
		if index > self.len {
			return None;
		}

		if index == 0 {
			return Some(self.push_front(val));
		}

		if index == self.len {
			return Some(self.push(val));
		}

		// FIXME: get rid of unbound lifetime
		let node = unsafe { unbound(self.get_node(index)) };

		Some(self.insert_before(val, node))
	}

	pub fn remove(&mut self, index: usize) -> Option<MemOwner<T>>
	{
		if index >= self.len {
			return None;
		}

		if index == 0 {
			return self.pop_front();
		}

		if index == self.len - 1 {
			return self.pop();
		}

		// FIXME: get rid of unbound lifetime
		let node = unsafe { unbound(self.get_node(index)) };

		Some(self.remove_node(node))
	}

	pub fn insert_before(&mut self, new_node: MemOwner<T>, node: &T) -> &mut T
	{
		assert!(self.len != 0);
		self.len += 1;

		let new_ptr = new_node.ptr_mut();

		if let Some(prev_node) = unsafe { node.prev_ptr().as_ref() } {
			new_node.set_prev(prev_node.as_mut_ptr());
			prev_node.set_next(new_ptr);
		} else {
			self.start = new_ptr;
			new_node.set_prev(null_mut());
		}

		node.set_prev(new_ptr);
		new_node.set_next(node.as_mut_ptr());

		new_node.leak()
	}

	pub fn insert_after(&mut self, new_node: MemOwner<T>, node: &T) -> &mut T
	{
		assert!(self.len != 0);
		self.len += 1;

		let new_ptr = new_node.ptr_mut();

		if let Some(next_node) = unsafe { node.next_ptr().as_ref() } {
			new_node.set_next(next_node.as_mut_ptr());
			next_node.set_prev(new_ptr);
		} else {
			self.end = new_ptr;
			new_node.set_next(null_mut());
		}

		node.set_next(new_ptr);
		new_node.set_prev(node.as_mut_ptr());

		new_node.leak()
	}

	// must pass in node that is in this list
	pub fn remove_node(&mut self, node: &T) -> MemOwner<T>
	{
		let prev = node.prev_ptr();
		let next = node.next_ptr();

		if prev.is_null() {
			self.start = next;
		} else {
			unsafe {
				prev.as_ref().unwrap().set_next(next);
			}
		}

		if next.is_null() {
			self.end = prev;
		} else {
			unsafe {
				next.as_ref().unwrap().set_prev(prev);
			}
		}

		self.len -= 1;

		unsafe { MemOwner::from_raw(node.as_mut_ptr()) }
	}

	pub fn update_node(&mut self, old: &T, new: MemOwner<T>)
	{
		let new_ptr = new.ptr_mut();

		if let Some(prev_node) = unsafe { old.prev_ptr().as_ref() } {
			prev_node.set_next(new_ptr);
			new.set_prev(prev_node.as_mut_ptr());
		} else {
			self.start = new_ptr;
			new.set_prev(null_mut());
		}

		if let Some(next_node) = unsafe { old.next_ptr().as_ref() } {
			next_node.set_prev(new_ptr);
			new.set_next(next_node.as_mut_ptr());
		} else {
			self.end = new_ptr;
			new.set_next(null_mut());
		}
	}

	// appends all elements from other linked list to this linked list
	pub fn append(&mut self, other: &mut LinkedList<T>)
	{
		if other.len() == 0 {
			return;
		}

		if self.len() == 0 {
			self.start = other.start;
			self.end = other.end;
			self.len = other.len;
		} else {
			unsafe {
				self.end.as_ref().unwrap().set_next(other.start);
				other.start.as_ref().unwrap().set_prev(self.end);
			}

			self.end = other.end;
		}

		other.start = null_mut();
		other.end = null_mut();
		other.len = 0;
	}

	pub fn get(&self, index: usize) -> Option<&T>
	{
		if index >= self.len {
			None
		} else {
			Some(self.get_node(index))
		}
	}

	pub fn get_mut(&mut self, index: usize) -> Option<&mut T>
	{
		if index >= self.len {
			None
		} else {
			Some(self.get_node_mut(index))
		}
	}

	pub fn iter(&self) -> Iter<'_, T>
	{
		Iter {
			start: self.start,
			end: self.end,
			len: self.len,
			marker: PhantomData,
		}
	}

	pub fn iter_mut(&mut self) -> IterMut<'_, T>
	{
		IterMut {
			start: self.start,
			end: self.end,
			len: self.len,
			marker: PhantomData,
		}
	}

	// must call with valid index
	fn get_node(&self, index: usize) -> &T
	{
		if index >= self.len {
			panic!("LinkedList internal error: get_node called with invalid index");
		}

		let mut node;
		if index * 2 > self.len {
			unsafe {
				node = self.end.as_ref().unwrap();
			}

			for _ in 0..(self.len - index - 1) {
				unsafe {
					node = node.prev_ptr().as_ref().unwrap();
				}
			}
		} else {
			unsafe {
				node = self.start.as_ref().unwrap();
			}

			for _ in 0..index {
				unsafe {
					node = node.next_ptr().as_ref().unwrap();
				}
			}
		}

		node
	}

	// must call with valid index
	fn get_node_mut(&mut self, index: usize) -> &mut T
	{
		if index >= self.len {
			panic!("LinkedList internal error: get_node_mut called with invalid index");
		}

		let mut node;
		if index * 2 > self.len {
			unsafe {
				node = self.end.as_mut().unwrap();
			}

			for _ in 0..(self.len - index - 1) {
				unsafe {
					node = node.prev_ptr().as_mut().unwrap();
				}
			}
		} else {
			unsafe {
				node = self.start.as_mut().unwrap();
			}

			for _ in 0..index {
				unsafe {
					node = node.next_ptr().as_mut().unwrap();
				}
			}
		}

		node
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

impl<'a, T: ListNode> IntoIterator for &'a LinkedList<T>
{
	type Item = &'a T;
	type IntoIter = Iter<'a, T>;

	fn into_iter(self) -> Self::IntoIter
	{
		self.iter()
	}
}

impl<'a, T: ListNode> IntoIterator for &'a mut LinkedList<T>
{
	type Item = &'a mut T;
	type IntoIter = IterMut<'a, T>;

	fn into_iter(self) -> Self::IntoIter
	{
		self.iter_mut()
	}
}

impl<T: ListNode + Debug> Debug for LinkedList<T>
{
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
	{
		f.debug_list().entries(self).finish().unwrap();
		Ok(())
	}
}

unsafe impl<T: ListNode + Send> Send for LinkedList<T> {}

// NOTE: it is safe to deallocate nodes returned from Iter and IterMut
pub struct Iter<'a, T: ListNode>
{
	start: *const T,
	end: *const T,
	len: usize,
	marker: PhantomData<&'a T>,
}

impl<'a, T: ListNode> Iterator for Iter<'a, T>
{
	type Item = &'a T;

	fn next(&mut self) -> Option<Self::Item>
	{
		if self.len == 0 {
			None
		} else {
			let out = unsafe { self.start.as_ref().unwrap() };
			self.start = out.next_ptr();
			self.len -= 1;
			Some(out)
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>)
	{
		(self.len, Some(self.len))
	}

	fn last(mut self) -> Option<Self::Item>
	{
		self.next_back()
	}
}

impl<'a, T: ListNode> DoubleEndedIterator for Iter<'a, T>
{
	fn next_back(&mut self) -> Option<Self::Item>
	{
		if self.len == 0 {
			None
		} else {
			let out = unsafe { self.end.as_ref().unwrap() };
			self.end = out.prev_ptr();
			self.len -= 1;
			Some(out)
		}
	}
}

impl<T: ListNode> ExactSizeIterator for Iter<'_, T> {}
impl<T: ListNode> core::iter::FusedIterator for Iter<'_, T> {}

pub struct IterMut<'a, T: ListNode>
{
	start: *mut T,
	end: *mut T,
	len: usize,
	marker: PhantomData<&'a T>,
}

impl<'a, T: ListNode> Iterator for IterMut<'a, T>
{
	type Item = &'a mut T;

	fn next(&mut self) -> Option<Self::Item>
	{
		if self.len == 0 {
			None
		} else {
			let out = unsafe { self.start.as_mut().unwrap() };
			self.start = out.next_ptr();
			self.len -= 1;
			Some(out)
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>)
	{
		(self.len, Some(self.len))
	}

	fn last(mut self) -> Option<Self::Item>
	{
		self.next_back()
	}
}

impl<'a, T: ListNode> DoubleEndedIterator for IterMut<'a, T>
{
	fn next_back(&mut self) -> Option<Self::Item>
	{
		if self.len == 0 {
			None
		} else {
			let out = unsafe { self.end.as_mut().unwrap() };
			self.end = out.prev_ptr();
			self.len -= 1;
			Some(out)
		}
	}
}

impl<T: ListNode> ExactSizeIterator for IterMut<'_, T> {}
impl<T: ListNode> core::iter::FusedIterator for IterMut<'_, T> {}
