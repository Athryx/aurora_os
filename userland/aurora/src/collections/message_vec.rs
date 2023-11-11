use core::alloc::Layout;
use core::fmt;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut, Index, IndexMut};
use core::ptr::{self, NonNull};
use core::slice::SliceIndex;
use core::cmp::max;
use core::mem::size_of;

use aser::ByteBuf;
use sys::MessageBuffer;
use bit_utils::Size;

use crate::allocator::allocator;

struct RawMessageVec<T> {
    ptr: NonNull<T>,
    cap: usize,
    message_buffer: Option<MessageBuffer>,
    marker: PhantomData<T>,
}

// code from rustonomicon
impl<T> RawMessageVec<T> {
    const fn new() -> Self {
        // !0 is usize::MAX. This branch should be stripped at compile time.
        let cap = if size_of::<T>() == 0 { !0 } else { 0 };

        // `NonNull::dangling()` doubles as "unallocated" and "zero-sized allocation"
        RawMessageVec {
            ptr: NonNull::dangling(),
            cap,
            message_buffer: None,
            marker: PhantomData,
        }
    }

    // tries to create a raw vec with specified capacity, returns out of mem on failure
    fn with_capacity(cap: usize) -> Self {
        if size_of::<T>() == 0 {
            RawMessageVec::new()
        } else {
            let layout = Layout::array::<T>(cap).unwrap();
            let (ptr, message_buffer) = allocator()
                .alloc_with_message_buffer(layout)
                .expect("MessageVec: out of mem");

            RawMessageVec {
                ptr: ptr.cast(),
                cap,
                message_buffer: Some(message_buffer),
                marker: PhantomData,
            }
        }
    }

    // returns out of mem on failure
    fn grow(&mut self, required_cap: Option<usize>) {
        // since we set the capacity to usize::MAX when T has size 0,
        // getting to here necessarily means the Vec is overfull.
        assert!(size_of::<T>() != 0, "capacity overflow");

        let mut new_cap = if self.cap == 0 {
            1
        } else {
            // This can't overflow because we ensure self.cap <= isize::MAX.
            2 * self.cap
        };

        // use required cap if it is larger than the 2 * current capacity
        if let Some(required_cap) = required_cap {
            assert!(required_cap <= isize::MAX as usize, "Allocation too large");

            // if required cap is less than current capacity, there is no need to grow
            if required_cap <= self.cap {
                return;
            }

            new_cap = max(new_cap, required_cap);
        }

        // `Layout::array` checks that the number of bytes is <= usize::MAX,
        // but this is redundant since old_layout.size() <= isize::MAX,
        // so the `unwrap` should never fail.
        let new_layout = Layout::array::<T>(new_cap).unwrap();

        // Ensure that the new allocation doesn't exceed `isize::MAX` bytes.
        assert!(new_layout.size() <= isize::MAX as usize, "Allocation too large");

        let new_alloc = if self.cap == 0 {
            allocator().alloc_with_message_buffer(new_layout)
        } else {
            let old_layout = Layout::array::<T>(self.cap).unwrap();
            unsafe { allocator().realloc_with_message_buffer(self.ptr.cast(), old_layout, new_layout) }
        };

        // If allocation fails, `new_ptr` will be null, in which case we abort.
        match new_alloc {
            Some((ptr, message_buffer)) => {
                self.ptr = ptr.as_non_null_ptr().cast();
                self.cap = new_cap;
                self.message_buffer = Some(message_buffer);
            },
            None => panic!("MessageVec: out of memory"),
        }
    }
}

impl<T> Drop for RawMessageVec<T> {
    fn drop(&mut self) {
        let elem_size = size_of::<T>();

        if self.cap != 0 && elem_size != 0 {
            let layout = Layout::array::<T>(self.cap).unwrap();
            unsafe {
                allocator().dealloc(self.ptr.cast(), layout);
            }
        }
    }
}

unsafe impl<T: Send> Send for RawMessageVec<T> {}
unsafe impl<T: Sync> Sync for RawMessageVec<T> {}

pub struct MessageVec<T> {
    inner: RawMessageVec<T>,
    len: usize,
}

impl<T> MessageVec<T> {
    pub const fn new() -> Self {
        MessageVec {
            inner: RawMessageVec::new(),
            len: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        MessageVec {
            inner: RawMessageVec::with_capacity(capacity),
            len: 0,
        }
    }

    // returns a mutable pointer to the object at the specified index
    unsafe fn off(&mut self, index: usize) -> *mut T {
        unsafe { self.as_mut_ptr().add(index) }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.inner.cap
    }

    pub fn message_buffer(&self) -> Option<MessageBuffer> {
        let mut buffer = self.inner.message_buffer?;
        // change buffer size to only include the piece of message vec
        // actually in use, not the total allocated region
        buffer.size = Size::from_bytes(size_of::<T>() * self.len);
        Some(buffer)
    }

    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    pub fn as_ptr(&self) -> *const T {
        self.inner.ptr.as_ptr() as *const T
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.inner.ptr.as_ptr()
    }

    pub fn as_slice(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len) }
    }

    pub fn get<I: SliceIndex<[T]>>(&self, index: I) -> Option<&I::Output> {
        index.get(self)
    }

    pub fn get_mut<I: SliceIndex<[T]>>(&mut self, index: I) -> Option<&mut I::Output> {
        index.get_mut(self)
    }

    pub fn push(&mut self, object: T) {
        if self.len == self.capacity() {
            self.inner.grow(None);
        }

        unsafe {
            ptr::write(self.off(self.len), object);
        }

        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;

            unsafe { Some(ptr::read(self.off(self.len))) }
        }
    }

    pub fn push_front(&mut self, object: T) {
        self.insert(0, object)
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.try_remove(0)
    }

    pub fn insert(&mut self, index: usize, object: T) {
        assert!(index <= self.len, "index out of bounds");

        if self.len == self.capacity() {
            self.inner.grow(None);
        }

        let ncpy = self.len - index;

        unsafe {
            ptr::copy(self.off(index), self.off(index + 1), ncpy);
            ptr::write(self.off(index), object);
        }

        self.len += 1;
    }

    pub fn remove(&mut self, index: usize) -> T {
        self.try_remove(index).expect("index out of bounds")
    }

    pub fn try_remove(&mut self, index: usize) -> Option<T> {
        if index >= self.len {
            return None;
        }

        let out = unsafe { ptr::read(self.off(index)) };

        self.len -= 1;
        let ncpy = self.len - index;

        unsafe {
            ptr::copy(self.off(index + 1), self.off(index), ncpy);
        }

        Some(out)
    }

    pub fn replace(&mut self, index: usize, object: T) -> T {
        assert!(index < self.len, "index out of bounds");

        let out = unsafe { ptr::read(self.off(index)) };

        unsafe {
            ptr::write(self.off(index), object);
        }

        out
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        if a == b {
            return;
        }

        let a_ptr = &mut self[a] as *mut T;
        let b_ptr = &mut self[b] as *mut T;

        // safety: a_ptr and b_ptr point to valid objects of T and do not overlap (but ptr::swap might not even care about that)
        unsafe { ptr::swap(a_ptr, b_ptr); }
    }

    pub fn iter(&self) -> Iter<T> {
        Iter {
            inner: RawIter::new(self.as_slice()),
            marker: PhantomData,
        }
    }

    pub fn iter_mut(&self) -> IterMut<T> {
        IterMut {
            inner: RawIter::new(self.as_slice()),
            marker: PhantomData,
        }
    }

    pub fn into_iter(self) -> IntoIter<T> {
        let raw_iter = RawIter::new(self.as_slice());

        // read raw vec and forget mem to take ownership of memory without dropping any elements
        // the remaining elements will be dropped by into iter
        let buffer = unsafe { ptr::read(&self.inner) };
        core::mem::forget(self);

        IntoIter {
            inner: raw_iter,
            _buffer: buffer,
        }
    }
}

impl<T: Clone> MessageVec<T> {
    pub fn extend_from_slice(&mut self, slice: &[T]) {
        self.inner.grow(Some(self.len + slice.len()));

        for item in slice {
            self.push(item.clone());
        }
    }
}

impl<T: Clone> MessageVec<T> {
    pub fn from_slice(slice: &[T]) -> Self {
        let mut out = Self::with_capacity(slice.len());

        for item in slice {
            out.push(item.clone());
        }

        out
    }
}

impl<T> Deref for MessageVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T> DerefMut for MessageVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T, I: SliceIndex<[T]>> Index<I> for MessageVec<T> {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        Index::index(self.as_slice(), index)
    }
}

impl<T, I: SliceIndex<[T]>> IndexMut<I> for MessageVec<T> {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        IndexMut::index_mut(self.as_mut_slice(), index)
    }
}

impl<T: fmt::Debug> fmt::Debug for MessageVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_slice(), f)
    }
}

/// Creates a new vec from the root allocator
/// 
/// This is mostly just used for bytebuf implementation
impl<T> Default for MessageVec<T> {
    fn default() -> Self {
        MessageVec::new()
    }
}

impl ByteBuf for MessageVec<u8> {
    fn push(&mut self, byte: u8) {
        self.push(byte);
    }

    fn extend_from_slice(&mut self, slice: &[u8]) {
        self.extend_from_slice(slice);
    }

    fn as_slice(&mut self) -> &mut [u8] {
        &mut self[..]
    }

    fn len(&self) -> usize {
        self.len()
    }
}

impl<T> Drop for MessageVec<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

#[derive(Clone)]
struct RawIter<T> {
    // inclusive
    start: usize,
    // exlusive
    end: usize,
    marker: PhantomData<*mut T>,
}

impl<T> RawIter<T> {
    fn new(data: &[T]) -> Self {
        let addr = data.as_ptr() as usize;

        RawIter {
            start: addr,
            end: addr + Self::elem_size() * data.len(),
            marker: PhantomData,
        }
    }

    fn elem_size() -> usize {
        if size_of::<T>() == 0 {
            1
        } else {
            size_of::<T>()
        }
    }
}

impl<T> Iterator for RawIter<T> {
    type Item = *mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start == self.end {
            None
        } else {
            let out = self.start as *mut T;
            // TODO: check if ok for zero size type
            self.start += Self::elem_size();
            Some(out)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let bound = (self.end - self.start) / Self::elem_size();
        (bound, Some(bound))
    }
}

impl<T> DoubleEndedIterator for RawIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start == self.end {
            None
        } else {
            self.start -= Self::elem_size();
            Some(self.start as *mut T)
        }
    }
}

impl<T> ExactSizeIterator for RawIter<T> {}
impl<T> FusedIterator for RawIter<T> {}

#[derive(Clone)]
pub struct Iter<'a, T: 'a> {
    inner: RawIter<T>,
    marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe { self.inner.next().map(|ptr| ptr.as_ref().unwrap()) }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> DoubleEndedIterator for Iter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        unsafe { self.inner.next_back().map(|ptr| ptr.as_ref().unwrap()) }
    }
}

impl<T> ExactSizeIterator for Iter<'_, T> {}
impl<T> FusedIterator for Iter<'_, T> {}

pub struct IterMut<'a, T: 'a> {
    inner: RawIter<T>,
    marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe { self.inner.next().map(|ptr| ptr.as_mut().unwrap()) }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> DoubleEndedIterator for IterMut<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        unsafe { self.inner.next_back().map(|ptr| ptr.as_mut().unwrap()) }
    }
}

impl<T> ExactSizeIterator for IterMut<'_, T> {}
impl<T> FusedIterator for IterMut<'_, T> {}

pub struct IntoIter<T> {
    inner: RawIter<T>,
    _buffer: RawMessageVec<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe { self.inner.next().map(|ptr| ptr::read(ptr)) }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        unsafe { self.inner.next_back().map(|ptr| ptr::read(ptr)) }
    }
}

impl<T> ExactSizeIterator for IntoIter<T> {}
impl<T> FusedIterator for IntoIter<T> {}

impl<T> Drop for IntoIter<T> {
    fn drop(&mut self) {
        // drop remaining elements
        while let Some(_) = self.next() {}
    }
}