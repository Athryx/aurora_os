use crate::prelude::*;

#[macro_export]
macro_rules! make_alloc_ref {
	($name:ident, $inner_name:ident, $trait:ident) => {
		#[derive(Clone)]
		enum $inner_name {
			Static(&'static dyn $trait),
			Raw(*const dyn $trait),
			CapAllocator($crate::container::Arc<$crate::alloc::cap_allocator::CapAllocator>),
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
				Self($inner_name::CapAllocator(arc))
			}

			// FIXME: find a better solution
			// safety: must have a shorter lifetime than allocer object
			pub unsafe fn new_raw(allocer: *const dyn $trait) -> Self {
				Self($inner_name::Raw(allocer))
			}

			/// Returns the allocator ths alloc ref is referencing
			pub fn allocator(&mut self) -> &dyn $trait {
				let new_inner = if let $inner_name::CapAllocator(ref allocer) = self.0 {
					if allocer.is_alive() {
						None
					} else {
						match allocer.get_closest_alive_parent() {
							$crate::alloc::CapAllocatorParent::Normal(allocer) => {
								Some($inner_name::CapAllocator(allocer))
							},
							$crate::alloc::CapAllocatorParent::Root(allocer) => {
								Some($inner_name::Static(allocer))
							}
						}
					}
				} else {
					None
				};

				if let Some(new_inner) = new_inner {
					self.0 = new_inner;
				}

				match self.0 {
					$inner_name::Static(allocer) => allocer,
					$inner_name::Raw(ptr) => unsafe { return ptr.as_ref().unwrap() },
					$inner_name::CapAllocator(ref allocer) => &**allocer,
				}
			}
		}
	};
}
