use core::prelude::v1::*;
pub use core::mem::size_of;
pub use core::marker::PhantomData;
pub use core::ptr::{self, null, null_mut};

pub use lazy_static::lazy_static;
pub use modular_bitfield::prelude::*;
pub use sys::SysErr;

pub use crate::misc::*;
pub use crate::arch::x64::bochs_break;
pub use crate::{print, println, eprint, eprintln, rprint, rprintln, init_array};
pub use crate::consts::PAGE_SIZE;
pub use crate::mem::{PhysAddr, VirtAddr, PhysRange, PhysRangeInner, UPhysRange, APhysRange, VirtRange, VirtRangeInner, UVirtRange, AVirtRange, phys_to_virt, virt_to_phys};
pub use crate::container::{Box, Vec};

pub type KResult<T> = Result<T, SysErr>;
