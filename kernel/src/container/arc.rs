use core::fmt;
use core::ops::Deref;
use core::ptr::NonNull;
use core::sync::atomic::{fence, AtomicUsize, Ordering};

use crate::alloc::OrigRef;
use crate::mem::HeapAllocation;
use crate::prelude::*;

const MAX_REFCOUNT: usize = isize::MAX as usize;

struct ArcInner<T: ?Sized> {
    strong: AtomicUsize,
    weak: AtomicUsize,

    allocer: OrigRef,
    allocation: Option<HeapAllocation>,

    data: T,
}

unsafe impl<T: ?Sized + Send + Sync> Send for ArcInner<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for ArcInner<T> {}

#[derive(Debug)]
pub struct Arc<T: ?Sized> {
    ptr: NonNull<ArcInner<T>>,
    phantom: PhantomData<ArcInner<T>>,
}

impl<T: ?Sized> Arc<T> {
    unsafe fn from_inner(ptr: NonNull<ArcInner<T>>) -> Self {
        Arc {
            ptr,
            phantom: PhantomData,
        }
    }

    unsafe fn from_ptr(ptr: *mut ArcInner<T>) -> Self {
        unsafe { Self::from_inner(NonNull::new_unchecked(ptr)) }
    }

    fn inner(&self) -> &ArcInner<T> {
        unsafe { self.ptr.as_ref() }
    }

    pub fn as_ptr(this: &Self) -> *const T {
        &this.inner().data as *const T
    }

    pub fn as_mut_ptr(this: &mut Self) -> *mut T {
        &this.inner().data as *const T as *mut T
    }

    pub fn downgrade(this: &Self) -> Weak<T> {
        this.inner().weak.fetch_add(1, Ordering::Relaxed);
        unsafe { Weak::from_ptr(this.ptr.as_ptr()) }
    }
}

impl<T> Arc<T> {
    pub fn new(data: T, mut allocer: OrigRef) -> KResult<Self> {
        let ptr = to_heap(
            ArcInner {
                strong: AtomicUsize::new(1),
                weak: AtomicUsize::new(1),
                allocer: allocer.clone(),
                allocation: None,
                data,
            },
            allocer.allocator(),
        )?;

        // meed to calculate this here becaust T is unsized in the Drop implementation
        // this could maybe be made different to get the HeapAllocation returned by the allocator so an OrigRef is not necessary
        let allocation = HeapAllocation::from_ptr(ptr);

        unsafe {
            ptr.as_mut().unwrap().allocation = Some(allocation);
        }

        unsafe { Ok(Self::from_ptr(ptr)) }
    }
}

impl<T: ?Sized> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner().data
    }
}

impl<T: ?Sized> Clone for Arc<T> {
    fn clone(&self) -> Self {
        // relaxed is ok because sending this arc to another thread must use synchronization
        let old_strong = self.inner().strong.fetch_add(1, Ordering::Relaxed);

        // this is to stop refcount overflows if Arcs are being cloned and then forgotten
        if old_strong > MAX_REFCOUNT {
            panic!("Arc ref count is to high");
        }

        unsafe { Arc::from_ptr(self.ptr.as_ptr()) }
    }
}

unsafe impl<#[may_dangle] T: ?Sized> Drop for Arc<T> {
    fn drop(&mut self) {
        // return early if no need to drop
        if self.inner().strong.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }

        // this fence synchronizes with the previouse release of reference count
        fence(Ordering::Acquire);

        // drop data referenced by Arc
        // safety: only the current arc can reference this data, since the strong count is at 0
        unsafe {
            ptr::drop_in_place(Arc::as_mut_ptr(self));
        }

        // drop the 1 weak reference that all the Arcs collectively own
        unsafe { drop(Weak::from_ptr(self.ptr.as_ptr())) }
    }
}

unsafe impl<T: ?Sized + Send + Sync> Send for Arc<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for Arc<T> {}

pub struct Weak<T: ?Sized> {
    ptr: NonNull<ArcInner<T>>,
}

impl<T: ?Sized> Weak<T> {
    unsafe fn from_inner(ptr: NonNull<ArcInner<T>>) -> Self {
        Weak {
            ptr,
        }
    }

    unsafe fn from_ptr(ptr: *mut ArcInner<T>) -> Self {
        unsafe { Self::from_inner(NonNull::new_unchecked(ptr)) }
    }

    fn inner(&self) -> &ArcInner<T> {
        unsafe { self.ptr.as_ref() }
    }

    pub fn as_ptr(&self) -> *const T {
        &self.inner().data as *const T
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        &self.inner().data as *const T as *mut T
    }

    pub fn upgrade(&self) -> Option<Arc<T>> {
        let mut strong_count = self.inner().strong.load(Ordering::Relaxed);

        loop {
            if strong_count == 0 {
                return None;
            }

            if strong_count > MAX_REFCOUNT {
                panic!("Weak ref count is to high");
            }

            match self.inner().strong.compare_exchange_weak(strong_count, strong_count + 1, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => unsafe {
                    return Some(Arc::from_ptr(self.ptr.as_ptr()));
                },
                Err(count) => strong_count = count,
            }
        }
    }
}

impl<T: ?Sized> Clone for Weak<T> {
    fn clone(&self) -> Self {
        // relaxed is ok because sending this weak to another thread must use synchronization
        let old_weak = self.inner().weak.fetch_add(1, Ordering::Relaxed);

        // this is to stop refcount overflows if Arcs are being cloned and then forgotten
        if old_weak > MAX_REFCOUNT {
            panic!("Weak ref count is to high");
        }

        unsafe { Weak::from_ptr(self.ptr.as_ptr()) }
    }
}

unsafe impl<#[may_dangle] T: ?Sized> Drop for Weak<T> {
    fn drop(&mut self) {
        // return early if no need to drop
        if self.inner().weak.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }

        // this fence synchronizes with the previouse release of reference count
        fence(Ordering::Acquire);

        // copy allocator bitwise out of inner, so we can deallocate inner before dropping allocator
        let mut allocer = unsafe { ptr::read(&self.inner().allocer) };

        // panic safety: all constructors will initilize this field to Some
        let allocation = self.inner().allocation.unwrap();

        unsafe {
            allocer.allocator().dealloc_orig(allocation);
        }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Weak<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(Weak)")
    }
}

unsafe impl<T: ?Sized + Send + Sync> Send for Weak<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for Weak<T> {}
