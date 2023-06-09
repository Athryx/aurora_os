use core::alloc::Layout;
use core::fmt;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut, Index, IndexMut};
use core::ptr::NonNull;
use core::slice::SliceIndex;
use core::cmp::max;

use aser::ByteBuf;

use crate::alloc::{HeapRef, root_alloc_ref};
use crate::prelude::*;

struct RawVec<T> {
    ptr: NonNull<T>,
    cap: usize,
    marker: PhantomData<T>,
    allocer: HeapRef,
}

// code from rustonomicon
impl<T> RawVec<T> {
    const fn new(allocer: HeapRef) -> Self {
        // !0 is usize::MAX. This branch should be stripped at compile time.
        let cap = if size_of::<T>() == 0 { !0 } else { 0 };

        // `NonNull::dangling()` doubles as "unallocated" and "zero-sized allocation"
        RawVec {
            ptr: NonNull::dangling(),
            cap,
            marker: PhantomData,
            allocer,
        }
    }

    // tries to create a raw vec with specified capacity, returns out of mem on failure
    fn try_with_capacity(mut allocer: HeapRef, cap: usize) -> KResult<Self> {
        if size_of::<T>() == 0 {
            Ok(RawVec::new(allocer))
        } else {
            let layout = Layout::array::<T>(cap).unwrap();
            let ptr = allocer
                .alloc(layout)
                .ok_or(SysErr::OutOfMem)?
                .cast();

            Ok(RawVec {
                ptr,
                cap,
                marker: PhantomData,
                allocer,
            })
        }
    }

    // returns out of mem on failure
    fn try_grow(&mut self, required_cap: Option<usize>) -> KResult<()> {
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
                return Ok(())
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
            self.allocer.alloc(new_layout)
        } else {
            let old_layout = Layout::array::<T>(self.cap).unwrap();
            unsafe { self.allocer.realloc(self.ptr.cast(), old_layout, new_layout) }
        };

        // If allocation fails, `new_ptr` will be null, in which case we abort.
        match new_alloc {
            Some(ptr) => {
                self.ptr = ptr.as_non_null_ptr().cast();
                self.cap = new_cap;
                Ok(())
            },
            None => Err(SysErr::OutOfMem),
        }
    }
}

impl<T> Drop for RawVec<T> {
    fn drop(&mut self) {
        let elem_size = size_of::<T>();

        if self.cap != 0 && elem_size != 0 {
            let layout = Layout::array::<T>(self.cap).unwrap();
            unsafe {
                self.allocer.dealloc(self.ptr.cast(), layout);
            }
        }
    }
}

unsafe impl<T: Send> Send for RawVec<T> {}
unsafe impl<T: Sync> Sync for RawVec<T> {}

pub struct Vec<T> {
    inner: RawVec<T>,
    len: usize,
}

impl<T> Vec<T> {
    pub const fn new(allocer: HeapRef) -> Self {
        Vec {
            inner: RawVec::new(allocer),
            len: 0,
        }
    }

    pub fn try_with_capacity(allocer: HeapRef, capacity: usize) -> KResult<Self> {
        Ok(Vec {
            inner: RawVec::try_with_capacity(allocer, capacity)?,
            len: 0,
        })
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

    pub fn allocator(&mut self) -> &mut HeapRef {
        &mut self.inner.allocer
    }

    pub fn alloc_ref(&self) -> HeapRef {
        self.inner.allocer.clone()
    }

    pub fn get<I: SliceIndex<[T]>>(&self, index: I) -> Option<&I::Output> {
        index.get(self)
    }

    pub fn get_mut<I: SliceIndex<[T]>>(&mut self, index: I) -> Option<&mut I::Output> {
        index.get_mut(self)
    }

    pub fn push(&mut self, object: T) -> KResult<()> {
        if self.len == self.capacity() {
            self.inner.try_grow(None)?;
        }

        unsafe {
            ptr::write(self.off(self.len), object);
        }

        self.len += 1;

        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;

            unsafe { Some(ptr::read(self.off(self.len))) }
        }
    }

    pub fn insert(&mut self, index: usize, object: T) -> KResult<()> {
        assert!(index <= self.len, "index out of bounds");

        if self.len == self.capacity() {
            self.inner.try_grow(None)?;
        }

        let ncpy = self.len - index;

        unsafe {
            ptr::copy(self.off(index), self.off(index + 1), ncpy);
            ptr::write(self.off(index), object);
        }

        self.len += 1;

        Ok(())
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

impl<T: Clone> Vec<T> {
    pub fn extend_from_slice(&mut self, slice: &[T]) -> KResult<()> {
        self.inner.try_grow(Some(self.len + slice.len()))?;

        for item in slice {
            // panic safety: we have already reserved enough space so this should not fail
            self.push(item.clone()).unwrap();
        }

        Ok(())
    }
}

impl<T: Clone> Vec<T> {
    pub fn from_slice(allocer: HeapRef, slice: &[T]) -> KResult<Self> {
        let mut out = Self::try_with_capacity(allocer, slice.len())?;

        for item in slice {
            out.push(item.clone())?;
        }

        Ok(out)
    }
}

impl<T> Deref for Vec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T> DerefMut for Vec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T, I: SliceIndex<[T]>> Index<I> for Vec<T> {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        Index::index(self.as_slice(), index)
    }
}

impl<T, I: SliceIndex<[T]>> IndexMut<I> for Vec<T> {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        IndexMut::index_mut(self.as_mut_slice(), index)
    }
}

impl<T: fmt::Debug> fmt::Debug for Vec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_slice(), f)
    }
}

/// Creates a new vec from the root allocator
/// 
/// This is mostly just used for bytebuf implementation
impl<T> Default for Vec<T> {
    fn default() -> Self {
        Vec::new(root_alloc_ref())
    }
}

impl ByteBuf for Vec<u8> {
    fn push(&mut self, byte: u8) {
        self.push(byte)
            .expect("failed to push byte to byte buffer");
    }

    fn extend_from_slice(&mut self, slice: &[u8]) {
        self.extend_from_slice(slice)
            .expect("failed to extend bytebuf from slice");
    }

    fn as_slice(&mut self) -> &mut [u8] {
        &mut self[..]
    }

    fn len(&self) -> usize {
        self.len()
    }
}

impl<T> Drop for Vec<T> {
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
    _buffer: RawVec<T>,
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