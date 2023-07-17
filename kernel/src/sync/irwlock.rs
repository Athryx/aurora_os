use core::ops::{Deref, DerefMut};

use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::arch::x64::IntDisable;

/// A RwLock that also disables interrupts when locked
#[derive(Debug)]
pub struct IrwLock<T: ?Sized>(RwLock<T>);

impl<T> IrwLock<T> {
    pub const fn new(user_data: T) -> Self {
        IrwLock(RwLock::new(user_data))
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }

    pub fn read(&self) -> IrwLockReadGuard<T> {
        let int_disable = IntDisable::new();
        IrwLockReadGuard(self.0.read(), int_disable)
    }

    pub fn try_read(&self) -> Option<IrwLockReadGuard<T>> {
        let int_disable = IntDisable::new();
        self.0.try_read().map(|guard| IrwLockReadGuard(guard, int_disable))
    }

    pub fn write(&self) -> IrwLockWriteGuard<T> {
        let int_disable = IntDisable::new();
        IrwLockWriteGuard(self.0.write(), int_disable)
    }

    pub fn try_write(&self) -> Option<IrwLockWriteGuard<T>> {
        let int_disable = IntDisable::new();
        self.0.try_write().map(|guard| IrwLockWriteGuard(guard, int_disable))
    }
}

impl<T: ?Sized + Default> Default for IrwLock<T> {
    fn default() -> IrwLock<T> {
        IrwLock::new(Default::default())
    }
}

unsafe impl<T: ?Sized + Send> Send for IrwLock<T> {}
unsafe impl<T: ?Sized + Send> Sync for IrwLock<T> {}

#[derive(Debug)]
pub struct IrwLockReadGuard<'a, T: ?Sized + 'a>(RwLockReadGuard<'a, T>, IntDisable);

impl<T> Deref for IrwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct IrwLockWriteGuard<'a, T: ?Sized + 'a>(RwLockWriteGuard<'a, T>, IntDisable);

impl<T> Deref for IrwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for IrwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
