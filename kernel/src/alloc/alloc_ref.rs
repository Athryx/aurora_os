use crate::prelude::*;

#[macro_export]
macro_rules! make_alloc_ref {
	($name:ident, $inner_name:ident, $trait:ident) => {
		#[derive(Clone)]
		enum $inner_name {
			Static(&'static dyn $trait),
			Raw(*const dyn $trait),
			OtherRc($crate::container::Arc<$crate::alloc::cap_allocator::CapAllocator>),
		}

		unsafe impl Send for $inner_name {}
		unsafe impl Sync for $inner_name {}

		impl core::fmt::Debug for $inner_name {
			fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
				writeln!(f, "(AllocRefInner)")
			}
		}

		/// A reference to a heap allocator
		#[derive(Debug, Clone)]
		pub struct $name($inner_name);

		impl $name {
			pub fn new(allocer: &'static dyn $trait) -> Self {
				Self($inner_name::Static(allocer))
			}

			pub fn from_arc(arc: $crate::container::Arc<$crate::alloc::cap_allocator::CapAllocator>) -> Self {
				Self($inner_name::OtherRc(arc))
			}

			// FIXME: find a better solution
			// safety: must have a shorter lifetime than allocer object
			pub unsafe fn new_raw(allocer: *const dyn $trait) -> Self {
				Self($inner_name::Raw(allocer))
			}

			pub fn allocator(&self) -> &dyn $trait {
				core::ops::Deref::deref(self)
			}
		}

		impl core::ops::Deref for $name {
			type Target = dyn $trait;

			fn deref(&self) -> &Self::Target {
				match self.0 {
					$inner_name::Static(allocer) => allocer,
					$inner_name::Raw(ptr) => unsafe { ptr.as_ref().unwrap() },
					$inner_name::OtherRc(ref allocer) => {
						todo!();
					},
				}
			}
		}
	};
}
