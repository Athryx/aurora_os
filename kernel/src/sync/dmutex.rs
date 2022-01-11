use core::sync::atomic::{AtomicBool, Ordering};
use core::ops::{Deref, DerefMut};

use spin::{Mutex, MutexGuard};

use crate::prelude::*;
use super::{IMutex, IMutexGuard};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DMutexErr {
	// the DMutex is already locked
	Locked,
	// the DMutex is dead
	Dead,
}

// a mutex which can be permanently disabled, and all waiting threads will unblock
#[derive(Debug)]
pub struct DMutex<T: ?Sized> {
	alive: AtomicBool,
	inner: Mutex<T>,
}

impl<T> DMutex<T> {
	pub const fn new(data: T) -> Self {
		DMutex {
			alive: AtomicBool::new(true),
			inner: Mutex::new(data),
		}
	}

	pub fn into_inner(self) -> T {
		self.inner.into_inner()
	}

	pub fn lock(&self) -> Result<DMutexGuard<T>, DMutexErr> {
		loop {
			if !self.alive.load(Ordering::Acquire) {
				return Err(DMutexErr::Dead);
			}

			if let Some(lock) = self.inner.try_lock() {
				return DMutexGuard::try_new(self, lock);
			}

			core::hint::spin_loop();
		}
	}

	pub fn try_lock(&self) -> Result<DMutexGuard<T>, DMutexErr> {
		if !self.alive.load(Ordering::Acquire) {
			return Err(DMutexErr::Dead);
		}

		if let Some(lock) = self.inner.try_lock() {
			DMutexGuard::try_new(self, lock)
		} else {
			Err(DMutexErr::Locked)
		}
	}

	pub fn is_alive(&self) -> bool {
		self.alive.load(Ordering::Acquire)
	}

	pub unsafe fn force_unlock(&self) {
		unsafe {
			self.inner.force_unlock()
		}
	}
}

impl<T: ?Sized + Default> Default for DMutex<T>
{
	fn default() -> DMutex<T>
	{
		DMutex::new(Default::default())
	}
}

unsafe impl<T: ?Sized + Send> Send for DMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for DMutex<T> {}

#[derive(Debug)]
pub struct DMutexGuard<'a, T: ?Sized + 'a> {
	dmutex: &'a DMutex<T>,
	inner: MutexGuard<'a, T>,
}

impl<'a, T> DMutexGuard<'a, T> {
	fn try_new(dmutex: &'a DMutex<T>, inner: MutexGuard<'a, T>) -> Result<DMutexGuard<'a, T>, DMutexErr> {
		// this is necessary to prevent race condition of alive being set to false after checking without the lock
		if !dmutex.alive.load(Ordering::Acquire) {
			return Err(DMutexErr::Dead);
		}

		Ok(DMutexGuard {
			dmutex,
			inner,
		})
	}

	pub fn destroy(self) {
		self.dmutex.alive.store(false, Ordering::Release);
	}
}

impl<T> Deref for DMutexGuard<'_, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<T> DerefMut for DMutexGuard<'_, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.inner
	}
}

#[derive(Debug)]
pub struct DIMutex<T: ?Sized> {
	alive: AtomicBool,
	inner: IMutex<T>,
}

impl<T> DIMutex<T> {
	pub const fn new(data: T) -> Self {
		DIMutex {
			alive: AtomicBool::new(true),
			inner: IMutex::new(data),
		}
	}

	pub fn into_inner(self) -> T {
		self.inner.into_inner()
	}

	pub fn lock(&self) -> Result<DIMutexGuard<T>, DMutexErr> {
		loop {
			if !self.alive.load(Ordering::Acquire) {
				return Err(DMutexErr::Dead);
			}

			if let Some(lock) = self.inner.try_lock() {
				return DIMutexGuard::try_new(self, lock);
			}

			core::hint::spin_loop();
		}
	}

	pub fn try_lock(&self) -> Result<DIMutexGuard<T>, DMutexErr> {
		if !self.alive.load(Ordering::Acquire) {
			return Err(DMutexErr::Dead);
		}

		if let Some(lock) = self.inner.try_lock() {
			DIMutexGuard::try_new(self, lock)
		} else {
			Err(DMutexErr::Locked)
		}
	}

	pub fn is_alive(&self) -> bool {
		self.alive.load(Ordering::Acquire)
	}

	pub unsafe fn force_unlock(&self) {
		unsafe {
			self.inner.force_unlock()
		}
	}
}

impl<T: ?Sized + Default> Default for DIMutex<T>
{
	fn default() -> DIMutex<T>
	{
		DIMutex::new(Default::default())
	}
}

unsafe impl<T: ?Sized + Send> Send for DIMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for DIMutex<T> {}

#[derive(Debug)]
pub struct DIMutexGuard<'a, T: ?Sized + 'a> {
	dmutex: &'a DIMutex<T>,
	inner: IMutexGuard<'a, T>,
}

impl<'a, T> DIMutexGuard<'a, T> {
	fn try_new(dmutex: &'a DIMutex<T>, inner: IMutexGuard<'a, T>) -> Result<DIMutexGuard<'a, T>, DMutexErr> {
		// this is necessary to prevent race condition of alive being set to false after checking without the lock
		if !dmutex.alive.load(Ordering::Acquire) {
			return Err(DMutexErr::Dead);
		}

		Ok(DIMutexGuard {
			dmutex,
			inner,
		})
	}

	pub fn destroy(self) {
		self.dmutex.alive.store(false, Ordering::Release);
	}
}

impl<T> Deref for DIMutexGuard<'_, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<T> DerefMut for DIMutexGuard<'_, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.inner
	}
}
