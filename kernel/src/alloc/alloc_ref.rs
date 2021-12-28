use crate::prelude::*;

#[macro_export]
macro_rules! make_alloc_ref {
	($name:ident, $inner_name:ident, $trait:ident) => {
		#[derive(Clone)]
		enum $inner_name {
			Static(&'static dyn $trait),
			Raw(*const dyn $trait),
			// uncomment once Arcs are addded
			//OtherRc(Arc<CapAllocator>),
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

			// FIXME: find a better solution
			// safety: object
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
				}
			}
		}
	};
}
