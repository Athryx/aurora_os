use core::cell::{Cell, RefCell};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::arch::asm;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::Any;

use super::Thread;

/// Stores all thread local variables
#[repr(C)]
pub struct ThreadLocalData {
    self_addr: AtomicUsize,
    pub(super) thread: Thread,
    currently_dropping: Cell<bool>,
    // TODO: find a faster way to do this, this might be a bit slow
    data: RefCell<Vec<Option<Rc<dyn Any>>>>,
}

impl ThreadLocalData {
    /// Initializes thread local data for the current thread
    pub fn init(thread: Thread) {
        let data = Box::new(ThreadLocalData {
            self_addr: AtomicUsize::new(0),
            thread,
            currently_dropping: Cell::new(false),
            data: RefCell::new(Vec::new()),
        });

        let data = Box::leak(data);
        let local_data_addr = data as *const ThreadLocalData as usize;
        data.self_addr.store(local_data_addr, Ordering::Relaxed);

        sys::Thread::set_local_pointer(local_data_addr);
    }

    /// # Safety
    /// 
    /// local data must have been initialized
    pub(super) unsafe fn get() -> *const Self {
        let local_data_addr: usize;
        unsafe {
            asm!(
                "mov {}, fs:0",
                out(reg) local_data_addr,
                options(nostack),
            );
        }
        local_data_addr as *const Self
    }

    /// # Safety
    /// 
    /// local data must have been initialized
    pub unsafe fn dealloc() {
        unsafe {
            drop(Box::from_raw(Self::get() as *mut Self));
        }
    }

    fn currently_dropping(&self) -> bool {
        self.currently_dropping.get()
    }

    /// Initializes the thread local variable at the given index if it is not initialized yet
    fn init_index<T: 'static>(&self, index: usize, init_fn: impl FnOnce() -> T) {
        if self.currently_dropping() {
            panic!("cannot initialize new tls slot while thread local data is being dropped");
        }

        let data = self.data.borrow();
        let elem = data.get(index)
            .map(Option::as_ref)
            .flatten();

        if elem.is_none() {
            // avoid holding borrowed refcell while calling init_fn so references
            // to other thread local variables doesn't cause a panic
            drop(data);

            let new_elem = Rc::new(init_fn());

            let mut data = self.data.borrow_mut();

            // fill vector with nones until index is valid
            while index >= data.len() {
                data.push(None);
            }

            data[index] = Some(new_elem);
        }
    }

    /// Gets the local data at `index` and calls `f` with the given value, and returns the result of f
    /// 
    /// Returns `None` if the slot at `index` is not initialized or if the value in the slot is the wrong type
    fn with_index<T: 'static, R>(&self, index: usize, f: impl FnOnce(&T) -> R) -> Option<R> {
        let elem = self.data.borrow()
            .get(index)?
            .as_ref()?
            .clone(); // clone out of the rc so if `f` calls with_index recursively no panic occurs

        Some(f(elem.downcast_ref::<T>()?))
    }
}

impl Drop for ThreadLocalData {
    fn drop(&mut self) {
        // technically at this point it is undefined behavior if a destructor references the thread local data,
        // but this will hopefully cause a panic instead of a segfault
        self.currently_dropping.set(true);
    }
}

static LOCAL_KEY_INDEX_COUNTER: AtomicUsize = AtomicUsize::new(0);

const UNINITIALIZED_INDEX: usize = 0xffffffffffffffff;
const CURRENTLY_INITIALIZING: usize = 0xfffffffffffffffd;

pub struct LocalKey<T: 'static> {
    index: AtomicUsize,
    init_fn: Option<fn() -> T>,
}

impl<T: 'static> LocalKey<T> {
    pub const fn new(init_fn: Option<fn() -> T>) -> Self {
        LocalKey {
            index: AtomicUsize::new(UNINITIALIZED_INDEX),
            init_fn,
        }
    }

    fn get_index(&self) -> usize {
        // relaxed ordering is fine here because these operations are not used to synchronize anything
        // the only thing that matters is the index is unique

        let num = self.index.load(Ordering::Relaxed);
        if num != CURRENTLY_INITIALIZING || num != UNINITIALIZED_INDEX {
            // fast path if index is already initialized
            return num;
        }

        match self.index.compare_exchange(UNINITIALIZED_INDEX, CURRENTLY_INITIALIZING, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => {
                // swap successful, it is our job to initialize index
                let key_index = LOCAL_KEY_INDEX_COUNTER.fetch_add(1, Ordering::Relaxed);
                self.index.store(key_index, Ordering::Relaxed);
                key_index
            },
            Err(num) => {
                if num == CURRENTLY_INITIALIZING {
                    loop {
                        let key_index = self.index.load(Ordering::Relaxed);
                        if key_index != CURRENTLY_INITIALIZING {
                            break key_index;
                        }

                        core::hint::spin_loop();
                    }
                } else {
                    // index is already initialized, return the index
                    num
                }
            }
        }
    }

    pub fn with<R, F: FnOnce(&T) -> R>(&self, f: F) -> R {
        // fixme: this is not actually safe, caller might call before thread local variable is initialized
        // this function is still marked as safe for compatability with rust std definition
        let local_data = unsafe {
            ThreadLocalData::get().as_ref().unwrap()
        };

        let index = self.get_index();

        if let Some(init_fn) = self.init_fn {
            local_data.init_index(index, init_fn);
        }

        local_data.with_index(self.get_index(), f)
            .expect("failed to get thread local variable")
    }

    pub fn init_with(&self, f: impl FnOnce() -> T) {
        // fixme: this is not actually safe, caller might call before thread local variable is initialized
        // this function is still marked as safe for compatability with rust std definition
        let local_data = unsafe {
            ThreadLocalData::get().as_ref().unwrap()
        };

        local_data.init_index(self.get_index(), f);
    }
}

/// Declares a thread local variable
// This is copied from rust standard library
#[macro_export]
macro_rules! thread_local {
    // empty (base case for the recursion)
    () => {};

    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = const { $init:expr }; $($rest:tt)*) => (
        $crate::thread_local_inner!($(#[$attr])* $vis $name, $t, const $init);
        $crate::thread_local!($($rest)*);
    );

    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = const { $init:expr }) => (
        $crate::thread_local_inner!($(#[$attr])* $vis $name, $t, const $init);
    );

    // process multiple declarations
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr; $($rest:tt)*) => (
        $crate::thread_local_inner!($(#[$attr])* $vis $name, $t, $init);
        $crate::thread_local!($($rest)*);
    );

    // handle a single declaration
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr) => (
        $crate::thread_local_inner!($(#[$attr])* $vis $name, $t, $init);
    );


    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty; $($rest:tt)*) => (
        $crate::thread_local_inner!($(#[$attr])* $vis $name, $t,);
        $crate::thread_local!($($rest)*);
    );

    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty) => (
        $crate::thread_local_inner!($(#[$attr])* $vis $name, $t,);
    );
}

#[macro_export]
macro_rules! thread_local_inner {
    (@key $t:ty, const $init:expr) => ({
        fn __init_thread_local() -> $t {
            const INIT_EXPR: $t = $init;
            INIT_EXPR
        }

        $crate::thread::LocalKey::new(Some(__init_thread_local))
    });

    (@key $t:ty, $init:expr) => ({
        fn __init_thread_local() -> $t {
            $init
        }

        $crate::thread::LocalKey::new(Some(__init_thread_local))
    });

    (@key $t:ty,) => ({
        $crate::thread::LocalKey::new(None)
    });

    ($(#[$attr:meta])* $vis:vis $name:ident, $t:ty, $($init:tt)*) => (
        $(#[$attr])* $vis static $name: $crate::thread::LocalKey<$t> =
            $crate::thread_local_inner!(@key $t, $($init)*);
    );
}