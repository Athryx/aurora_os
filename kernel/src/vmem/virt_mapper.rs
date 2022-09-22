use alloc::collections::BTreeMap;

use bitflags::bitflags;

use crate::uses::*;
use crate::arch::x64::{get_cr3, invlpg, set_cr3};
use crate::consts;
use crate::cap::CapFlags;
use crate::util::{Futex, FutexGuard};
use crate::syscall::udata::UserPageArray;
use crate::sched::SpawnMapFlags;
use super::phys_alloc::{zm, Allocation, ZoneManager};
use super::error::MemErr;
use super::*;

const PAGE_ADDR_BITMASK: usize = 0x000ffffffffff000;
lazy_static! {
	static ref MAX_MAP_ADDR: usize = consts::KERNEL_VIRT_RANGE.as_usize ();

	// TODO: make global
	static ref HIGHER_HALF_PAGE_POINTER: PageTablePointer = PageTablePointer::new (*consts::KZONE_PAGE_TABLE_POINTER,
		PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::GLOBAL);

	// default page tableflags for any pages that map another page, these are the most permissive flags, and should be overriden by the final page
	static ref PARENT_FLAGS: PageTableFlags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER;
}

pub type FAllocerType = ZoneManager;

pub unsafe trait FrameAllocator
{
	// implementor must guarentee that constructing a new allocation with same address and size of 1 page will work to free
	fn alloc_frame(&self) -> Allocation;
	unsafe fn dealloc_frame(&self, frame: Allocation);
}

bitflags! {
	struct PageTableFlags: usize
	{
		const NONE = 		0;
		const PRESENT = 	1;
		const WRITABLE = 	1 << 1;
		const USER = 		1 << 2;
		const PWT = 		1 << 3;
		const PCD = 		1 << 4;
		const ACCESSED = 	1 << 5;
		const DIRTY = 		1 << 6;
		const HUGE = 		1 << 7;
		const GLOBAL = 		1 << 8;
		const NO_EXEC =		1 << 63;
	}
}

impl PageTableFlags
{
	fn from_mapping_flags(flags: PageMappingFlags) -> Self
	{
		let mut out = PageTableFlags::NONE;
		if flags.contains(PageMappingFlags::WRITE) {
			out |= PageTableFlags::WRITABLE;
		}

		if !flags.contains(PageMappingFlags::EXEC) {
			out |= PageTableFlags::NO_EXEC;
		}

		if flags.exists() {
			out |= PageTableFlags::PRESENT;
		}

		if flags.contains(PageMappingFlags::USER) {
			out |= PageTableFlags::USER;
		}

		out
	}

	fn present(&self) -> bool
	{
		self.contains(Self::PRESENT)
	}
}

bitflags! {
	pub struct PageMappingFlags: usize
	{
		const NONE =		0;
		const READ =		1;
		const WRITE =		1 << 1;
		const EXEC =		1 << 2;
		const USER = 		1 << 3;
		const EXACT_SIZE =	1 << 4;
	}
}

impl PageMappingFlags
{
	pub fn from_cap_flags(flags: CapFlags) -> Self {
		let mut out = PageMappingFlags::USER;
		if flags.contains(CapFlags::READ) {
			out |= PageMappingFlags::READ;
		}

		if flags.contains(CapFlags::WRITE) {
			out |= PageMappingFlags::WRITE;
		}

		out
	}

	fn exists(&self) -> bool
	{
		self.intersects(PageMappingFlags::READ | PageMappingFlags::WRITE | PageMappingFlags::EXEC)
	}
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
struct PageTablePointer(usize);

impl PageTablePointer
{
	fn new(addr: PhysAddr, flags: PageTableFlags) -> Self
	{
		let addr = addr.as_u64() as usize;
		PageTablePointer(addr | flags.bits())
	}

	unsafe fn as_ref<'a, 'b>(&'a self) -> Option<&'b PageTable>
	{
		if self.0 & PageTableFlags::PRESENT.bits() == 0 {
			None
		} else {
			let addr = phys_to_virt(PhysAddr::new((self.0 & PAGE_ADDR_BITMASK) as u64));
			Some((addr.as_u64() as *const PageTable).as_ref().unwrap())
		}
	}

	unsafe fn as_mut<'a, 'b>(&'a mut self) -> Option<&'b mut PageTable>
	{
		if self.0 & PageTableFlags::PRESENT.bits() == 0 {
			None
		} else {
			let addr = phys_to_virt(PhysAddr::new((self.0 & PAGE_ADDR_BITMASK) as u64));
			Some((addr.as_u64() as *mut PageTable).as_mut().unwrap())
		}
	}

	fn flags(&self) -> PageTableFlags
	{
		PageTableFlags::from_bits_truncate(self.0)
	}

	unsafe fn set_flags(&mut self, flags: PageTableFlags)
	{
		self.0 = (self.0 & PAGE_ADDR_BITMASK) | flags.bits();
	}
}

#[repr(transparent)]
#[derive(Debug)]
struct PageTable([PageTablePointer; PAGE_SIZE / 8]);

impl PageTable
{
	fn new<T: FrameAllocator>(
		allocer: &T,
		flags: PageTableFlags,
		dropable: bool,
	) -> PageTablePointer
	{
		let frame = allocer.alloc_frame().as_usize();
		unsafe {
			memset(frame as *mut u8, PAGE_SIZE, 0);
		}

		let addr = virt_to_phys_usize(frame);
		let flags = flags | PageTableFlags::PRESENT;
		let mut out = PageTablePointer(addr | flags.bits());
		if !dropable {
			unsafe {
				out.as_mut().unwrap().set_count(1);
			}
		}
		out
	}

	fn count(&self) -> usize
	{
		get_bits(self.0[0].0, 52..63)
	}

	fn set_count(&mut self, n: usize)
	{
		let n = get_bits(n, 0..11);
		let ptr_no_count = self.0[0].0 & 0x800fffffffffffff;
		self.0[0] = PageTablePointer(ptr_no_count | (n << 52));
	}

	fn inc_count(&mut self, n: isize)
	{
		self.set_count((self.count() as isize + n) as _);
	}

	fn present(&self, index: usize) -> bool
	{
		(self.0[index].0 & PageTableFlags::PRESENT.bits()) != 0
	}

	// TODO: make this more safe
	unsafe fn free_if_empty<'a, T: FrameAllocator + 'a>(&mut self, allocer: &'a T) -> bool
	{
		if self.count() == 0 {
			self.dealloc(allocer);
			true
		} else {
			false
		}
	}

	unsafe fn dealloc<'a, T: FrameAllocator + 'a>(&mut self, allocer: &'a T)
	{
		let frame = Allocation::new(self.addr(), PAGE_SIZE);
		allocer.dealloc_frame(frame);
	}

	unsafe fn dealloc_all<'a, T: FrameAllocator + 'a>(&mut self, allocer: &'a T)
	{
		self.dealloc_recurse(allocer, 3);
	}

	unsafe fn dealloc_recurse<'a, T: FrameAllocator + 'a>(&mut self, allocer: &'a T, level: usize)
	{
		if level > 0 {
			for pointer in self.0.iter_mut() {
				if pointer.flags().contains(PageTableFlags::HUGE) {
					continue;
				}

				match pointer.as_mut() {
					Some(page_table) => {
						if level > 0 {
							page_table.dealloc_recurse(allocer, level - 1);
						}
					},
					None => continue,
				}
			}
		}

		self.dealloc(allocer)
	}

	fn set(&mut self, index: usize, ptr: PageTablePointer)
	{
		assert!(!self.present(index));
		self.0[index] = ptr;
		self.inc_count(1);
	}

	fn get<'a, 'b>(&'a mut self, index: usize) -> &'b mut PageTable
	{
		unsafe { self.0[index].as_mut().unwrap() }
	}

	fn get_or_alloc<'a, 'b, T: FrameAllocator + 'a>(
		&'a mut self,
		index: usize,
		allocer: &'b T,
		flags: PageTableFlags,
	) -> &'a mut PageTable
	{
		if self.present(index) {
			unsafe { self.0[index].as_mut().unwrap() }
		} else {
			let mut out = PageTable::new(allocer, flags, true);
			self.set(index, out);
			unsafe { out.as_mut().unwrap() }
		}
	}

	// returns true if dropped
	unsafe fn remove<T: FrameAllocator>(&mut self, index: usize, allocer: &T) -> bool
	{
		let n = self.0[index].0;
		if self.present(index) {
			self.0[index] = PageTablePointer(n & !PageTableFlags::PRESENT.bits());
			self.inc_count(-1);
			self.free_if_empty(allocer)
		} else {
			false
		}
	}

	fn addr(&self) -> usize
	{
		self as *const _ as usize
	}
}

#[derive(Debug, Clone, Copy)]
enum VirtLayoutElementType
{
	Mem(PhysRange),
	// will translate this to physical address
	AllocedMem(Allocation),
	Empty(usize),
}

impl VirtLayoutElementType
{
	fn size(&self) -> usize
	{
		match self {
			Self::Mem(mem) => mem.size(),
			Self::AllocedMem(mem) => mem.len(),
			Self::Empty(size) => *size,
		}
	}
}

#[derive(Debug, Clone, Copy)]
pub struct VirtLayoutElement
{
	// internal data guarunteed to be page alligned
	phys_data: VirtLayoutElementType,
	// guarunteed to be page aligned
	map_size: usize,
	flags: PageTableFlags,
	mapping_flags: PageMappingFlags,
}

impl VirtLayoutElement
{
	// size is aligned up
	pub fn new(size: usize, flags: PageMappingFlags) -> Option<Self>
	{
		let size = align_up(size, PAGE_SIZE);

		let phys_data;
		let map_size;

		if flags.exists() {
			let mem = zm.alloc(size)?;

			phys_data = VirtLayoutElementType::AllocedMem(mem);

			map_size = if flags.contains(PageMappingFlags::EXACT_SIZE) {
				size
			} else {
				mem.len()
			};
		} else {
			phys_data = VirtLayoutElementType::Empty(size);
			map_size = size;
		}

		Some(VirtLayoutElement {
			phys_data,
			map_size,
			flags: PageTableFlags::from_mapping_flags(flags),
			mapping_flags: flags,
		})
	}

	// size is only used if the exact_size flag is set
	// size is aligned up
	pub fn from_mem(mem: Allocation, size: usize, flags: PageMappingFlags) -> Self
	{
		let size = align_up(size, PAGE_SIZE);

		VirtLayoutElement {
			phys_data: VirtLayoutElementType::AllocedMem(mem),
			map_size: if flags.contains(PageMappingFlags::EXACT_SIZE) {
				min(mem.len(), size)
			} else {
				mem.len()
			},
			flags: PageTableFlags::from_mapping_flags(flags),
			mapping_flags: flags,
		}
	}

	pub fn from_range(phys_range: PhysRange, flags: PageMappingFlags) -> Self
	{
		let phys_range = phys_range.aligned();
		VirtLayoutElement {
			phys_data: VirtLayoutElementType::Mem(phys_range),
			map_size: phys_range.size(),
			flags: PageTableFlags::from_mapping_flags(flags),
			mapping_flags: flags,
		}
	}

	pub fn size(&self) -> usize
	{
		self.map_size
	}

	pub fn raw_size(&self) -> usize
	{
		self.phys_data.size()
	}

	fn get_take_size(&mut self) -> Option<PageSize>
	{
		let psize = match self.phys_data {
			VirtLayoutElementType::AllocedMem(mem) => {
				let prange = mem.into();
				self.phys_data = VirtLayoutElementType::Mem(prange);
				prange.get_take_size()
			},
			VirtLayoutElementType::Mem(mem) => mem.get_take_size(),
			VirtLayoutElementType::Empty(mem) => {
				PageSize::try_from_usize(align_down_to_page_size(mem))
			},
		}?;

		let psize2 = PageSize::try_from_usize(align_down_to_page_size(self.map_size))?;

		Some(min(psize, psize2))
	}

	fn take(&mut self, size: PageSize) -> Option<(PhysFrame, PageTableFlags)>
	{
		let mut flags = self.flags;

		let pframe = match self.phys_data {
			VirtLayoutElementType::Mem(ref mut mem) => mem.take(size)?,
			VirtLayoutElementType::Empty(ref mut mem) => {
				if size as usize > *mem {
					return None;
				}
				*mem -= size as usize;
				flags = PageTableFlags::NONE;
				PhysFrame::new(PhysAddr::new(0), size)
			},
			VirtLayoutElementType::AllocedMem(mem) => {
				let mut prange = PhysRange::from(mem);
				let frame = prange.take(size)?;
				self.phys_data = VirtLayoutElementType::Mem(prange);
				frame
			},
		};

		self.map_size -= size as usize;

		Some((pframe, flags))
	}

	// returns some only if type is not empty and the mapping flags say present
	fn as_phys_zone(&self) -> Option<PhysRange>
	{
		if !self.flags.present() {
			return None;
		}

		match self.phys_data {
			// they should already be aligned, so new is fine
			VirtLayoutElementType::AllocedMem(mem) => {
				Some(PhysRange::new(virt_to_phys(mem.addr()), self.map_size))
			},
			VirtLayoutElementType::Mem(mem) => Some(PhysRange::new(mem.addr(), self.map_size)),
			VirtLayoutElementType::Empty(_) => None,
		}
	}

	pub unsafe fn dealloc(&self)
	{
		if let VirtLayoutElementType::AllocedMem(mem) = self.phys_data {
			zm.dealloc(mem);
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocType
{
	VirtMem,
	// TODO: probably remove
	PhysMap,
	// any capability managed mem
	Shared,
	Protected,
}

#[derive(Debug, Clone)]
pub struct VirtLayout
{
	data: Vec<VirtLayoutElement>,
	dealloc_que: Vec<VirtLayoutElement>,
	dirty_index: usize,
	clean_size: usize,
	atype: AllocType,
}

impl VirtLayout
{
	pub fn new(atype: AllocType) -> Self
	{
		VirtLayout {
			data: Vec::new(),
			dealloc_que: Vec::new(),
			dirty_index: 0,
			clean_size: 0,
			atype,
		}
	}

	pub fn from(vec: Vec<VirtLayoutElement>, atype: AllocType) -> Self
	{
		VirtLayout {
			data: vec,
			dealloc_que: Vec::new(),
			dirty_index: 0,
			clean_size: 0,
			atype,
		}
	}

	// TODO: probably not necesarry
	pub fn try_from(vec: Vec<VirtLayoutElement>, atype: AllocType) -> Option<Self>
	{
		if vec.is_empty() {
			None
		} else {
			Some(VirtLayout {
				data: vec,
				dealloc_que: Vec::new(),
				dirty_index: 0,
				clean_size: 0,
				atype,
			})
		}
	}

	pub fn alloc_type(&self) -> AllocType
	{
		self.atype
	}

	/*pub fn set_alloc_type (&mut self, atype: AllocType)
	{
		self.atype = atype;
	}*/

	pub fn push(&mut self, elem: VirtLayoutElement)
	{
		self.data.push(elem);
	}

	pub fn pop_delete(&mut self)
	{
		if let Some(elem) = self.data.pop() {
			if self.data.len() < self.dirty_index {
				self.dealloc_que.push(elem);
				self.dirty_index = self.data.len();
			}
		}
	}

	pub fn size(&self) -> usize
	{
		self.data.iter().fold(0, |n, a| n + a.size())
	}

	pub fn clean_slice(&self) -> &[VirtLayoutElement]
	{
		&self.data[..self.dirty_index]
	}

	pub fn dirty_slice(&self) -> &[VirtLayoutElement]
	{
		&self.data[self.dirty_index..]
	}

	pub fn clean_size(&self) -> usize
	{
		self.clean_size
	}

	// retuns none if virtlayout is completely empty (including dirty memory)
	pub fn flags(&self) -> Option<PageMappingFlags>
	{
		if let Some(elem) = self.data.get(0) {
			Some(elem.mapping_flags)
		} else {
			self.dealloc_que.get(0).map(|elem| elem.mapping_flags)
		}
	}

	// must only be called once
	pub unsafe fn dealloc(&self)
	{
		for a in self.data.iter() {
			a.dealloc()
		}

		for a in self.dealloc_que.iter() {
			a.dealloc()
		}
	}

	// should be called after unmapping part of virt layout
	unsafe fn sync_mem(&mut self)
	{
		for a in &self.data[self.dirty_index..] {
			self.clean_size += a.size();
		}
		self.dirty_index = self.data.len();

		for a in self.dealloc_que.iter() {
			self.clean_size -= a.size();
			a.dealloc();
		}
		self.dealloc_que.clear();
	}

	// touches all memory zones, marking them to be mapped again
	unsafe fn mark_unmapped(&mut self)
	{
		self.dirty_index = 0;
		self.clean_size = 0;

		for a in self.dealloc_que.iter() {
			a.dealloc();
		}
		self.dealloc_que.clear();
	}

	// reverts unsynced changes and deallocates their backing resources
	unsafe fn revert(&mut self)
	{
		while self.data.len() > self.dirty_index {
			self.data.pop().unwrap().dealloc();
		}

		for a in self.dealloc_que.iter().rev() {
			self.data.push(*a);
		}
		self.dealloc_que.clear();
	}
}

impl Drop for VirtLayout
{
	fn drop(&mut self)
	{
		unsafe {
			self.dealloc();
		}
	}
}

#[derive(Debug, Clone, Copy)]
enum PageMappingAction
{
	Map(PhysFrame, VirtFrame, PageTableFlags),
	Unmap(VirtFrame),
	//SetFlags(VirtFrame, PageTableFlags),
}

impl PageMappingAction
{
	fn virt_frame(&self) -> VirtFrame
	{
		match self {
			Self::Map(_, vframe, _) => *vframe,
			Self::Unmap(vframe) => *vframe,
		}
	}
}

// FIXME: this might not be the most elegant way to do it
// FIXME: wierd name
#[derive(Debug, Clone, Copy)]
struct Pmit
{
	// virt_zone and phys_zone lengths are guarunteed to be the same
	phys_zone: PhysRange,
	virt_zone: VirtRange,
	flags: PageTableFlags,
}

impl Pmit
{
	fn new(phys_zone: PhysRange, virt_zone: VirtRange, flags: PageTableFlags) -> Self
	{
		assert_eq!(phys_zone.size(), virt_zone.size());
		Pmit {
			phys_zone,
			virt_zone,
			flags,
		}
	}

	fn present(&self) -> bool
	{
		self.flags.present()
	}

	fn get_take_size(&self) -> Option<PageSize>
	{
		Some(min(
			self.phys_zone.get_take_size()?,
			self.virt_zone.get_take_size()?,
		))
	}

	fn take(&mut self, size: PageSize) -> Option<(PhysFrame, VirtFrame)>
	{
		let take_size = self.get_take_size()?;
		if size > take_size {
			None
		} else {
			Some((
				self.phys_zone.take(size).unwrap(),
				self.virt_zone.take(size).unwrap(),
			))
		}
	}
}

#[derive(Debug)]
struct PageMappingIterator
{
	// if flags are none, action is unmap
	zones: Vec<Pmit>,
	pindex: usize,
}

impl PageMappingIterator
{
	fn new(phys_zone: &VirtLayout, virt_zone: &VirtRange) -> Self
	{
		let mut zones = Vec::new();

		let mut vaddr = virt_zone.addr() + phys_zone.clean_size();

		// unmap first
		for a in &phys_zone.dealloc_que {
			vaddr -= a.size();

			if let Some(prange) = a.as_phys_zone() {
				let vrange = VirtRange::new(vaddr, a.size());
				zones.push(Pmit::new(prange, vrange, PageTableFlags::NONE));
			}
		}

		for a in phys_zone.dirty_slice() {
			if let Some(prange) = a.as_phys_zone() {
				let vrange = VirtRange::new(vaddr, a.size());
				zones.push(Pmit::new(prange, vrange, a.flags));
			}

			vaddr += a.size()
		}

		PageMappingIterator {
			zones,
			pindex: 0,
		}
	}

	fn new_unmapper(phys_zone: &VirtLayout, virt_zone: &VirtRange) -> Self
	{
		let mut zones = Vec::new();

		let mut vaddr = virt_zone.addr();

		for a in phys_zone.clean_slice() {
			if let Some(prange) = a.as_phys_zone() {
				let vrange = VirtRange::new(vaddr, a.size());
				zones.push(Pmit::new(prange, vrange, PageTableFlags::NONE));
			}

			vaddr += a.size()
		}

		// unmap first
		for a in phys_zone.dealloc_que.iter().rev() {
			if let Some(prange) = a.as_phys_zone() {
				let vrange = VirtRange::new(vaddr, a.size());
				zones.push(Pmit::new(prange, vrange, PageTableFlags::NONE));
			}

			vaddr += a.size();
		}

		PageMappingIterator {
			zones,
			pindex: 0,
		}
	}
}

impl Iterator for PageMappingIterator
{
	type Item = PageMappingAction;

	// returns vframe with VirtAddr == 0 if mem should not be mapped, just reserved
	fn next(&mut self) -> Option<Self::Item>
	{
		if self.pindex == self.zones.len() {
			return None;
		}

		// to make borrow checker happy
		let zlen = self.zones.len();
		let mut pmit = &mut self.zones[self.pindex];

		let size = loop {
			let size = pmit.get_take_size();

			if let Some(size) = size {
				break size;
			} else {
				self.pindex += 1;

				if self.pindex == zlen {
					return None;
				}

				pmit = &mut self.zones[self.pindex];
			}
		};

		let (pframe, vframe) = pmit.take(size).unwrap();

		let flags = pmit.flags;

		if flags.present() {
			Some(PageMappingAction::Map(pframe, vframe, flags))
		} else {
			Some(PageMappingAction::Unmap(vframe))
		}
	}
}

#[derive(Debug)]
pub struct VirtMapper<T: FrameAllocator + 'static>
{
	virt_map: Futex<BTreeMap<VirtRange, VirtLayout>>,
	cr3: Futex<PageTablePointer>,
	// in order to avoid race condition
	cr3_addr: usize,
	frame_allocer: &'static T,
}

impl<T: FrameAllocator> VirtMapper<T>
{
	// TODO: lazy tlb flushing
	pub fn new(frame_allocer: &'static T) -> VirtMapper<T>
	{
		let mut pml4_table = PageTable::new(frame_allocer, PageTableFlags::NONE, false);
		// NOTE: change index if kernel_vma changes
		unsafe {
			pml4_table
				.as_mut()
				.unwrap()
				.set(511, *HIGHER_HALF_PAGE_POINTER);
		}
		VirtMapper {
			virt_map: Futex::new(BTreeMap::new()),
			cr3: Futex::new(pml4_table),
			cr3_addr: pml4_table.0,
			frame_allocer,
		}
	}

	pub fn set_frame_allocator(&mut self, frame_allocer: &'static T)
	{
		self.frame_allocer = frame_allocer;
	}

	pub unsafe fn load(&self)
	{
		set_cr3(self.cr3_addr);
	}

	pub fn is_loaded(&self) -> bool
	{
		self.cr3_addr == get_cr3()
	}

	pub fn get_cr3(&self) -> usize {
		self.cr3_addr
	}

	pub fn get_mapped_range(&self, addr: VirtAddr) -> Option<VirtRange>
	{
		self.address_map(addr, |zone, _| {
			if zone.contains(addr) {
				Some(*zone)
			} else {
				None
			}
		})
	}

	pub fn get_alloc_type(&self, addr: VirtAddr) -> Option<AllocType>
	{
		self.address_map(addr, |_, zone| Some(zone.alloc_type()))
	}

	pub fn copy_to_allocation(&self, virt_zone: VirtRange) -> Option<Allocation>
	{
		if !virt_zone.is_aligned() {
			None
		} else {
			self.range_map(virt_zone, |mem| {
				let mut alloc = zm.alloc(mem.len())?;
				alloc.copy_from_mem(mem);
				Some(alloc)
			})
		}
	}

	pub fn range_map<F, U>(&self, virt_zone: VirtRange, f: F) -> Option<U>
	where
		F: FnOnce(&[u8]) -> Option<U>,
	{
		let btree = self.virt_map.lock();
		let mut prev_iter = btree.range(..virt_zone);
		let prev = prev_iter.next_back();

		let mut next_iter = btree.range(virt_zone..);
		let next = next_iter.next();

		if optac(prev, |p| p.0.full_contains_range(virt_zone))
			|| optac(next, |n| n.0.full_contains_range(virt_zone))
		{
			return f(unsafe { virt_zone.as_slice() });
		}

		if prev.is_some() && next.is_some() {
			let prev = prev.unwrap().0;
			let next = next.unwrap().0;
			if let Some(range) = prev.merge(*next) {
				if range.full_contains_range(virt_zone) {
					return f(unsafe { virt_zone.as_slice() });
				}
			}
		}

		None
	}

	pub fn address_map<F, U>(&self, addr: VirtAddr, f: F) -> Option<U>
	where
		F: FnOnce(&VirtRange, &VirtLayout) -> Option<U>,
	{
		let virt_zone = VirtRange::new_unaligned(addr, usize::MAX);

		let btree = self.virt_map.lock();
		let (range, layout) = btree.range(..virt_zone).next_back()?;
		f(range, layout)
	}

	fn contains(
		btree: &mut FutexGuard<BTreeMap<VirtRange, VirtLayout>>,
		virt_zone: VirtRange,
	) -> bool
	{
		btree.get(&virt_zone).is_some()
	}

	// find virt range of size size
	fn find_range(
		btree: &FutexGuard<BTreeMap<VirtRange, VirtLayout>>,
		size: usize,
	) -> Option<VirtRange>
	{
		// leave page at 0 empty so null pointers will page fault
		let mut laddr = PAGE_SIZE;
		let mut found = false;

		for zone in btree.keys() {
			let diff = zone.as_usize() - laddr;
			if diff >= size {
				found = true;
				break;
			}
			laddr = zone.as_usize() + zone.size();
		}

		if !found && (*MAX_MAP_ADDR - laddr < size) {
			return None;
		}

		Some(VirtRange::new(VirtAddr::new(laddr as _), size))
	}

	// get free space to left and right of virt_zone in bytes
	// if there is interference to left and right of virt_zone, returns none
	// pass with inclusive true to ensure virt_zone is not already inserted
	// if it is inserted, pass with inclusive false
	fn free_space(
		btree: &FutexGuard<BTreeMap<VirtRange, VirtLayout>>,
		virt_zone: VirtRange,
		exclude: Option<VirtRange>,
	) -> Option<(usize, usize)>
	{
		let mut prev_iter = btree.range(..virt_zone);
		let mut prev = prev_iter.next_back();

		let mut next_iter = btree.range(virt_zone..);
		let mut next = next_iter.next();

		if let Some(exclude) = exclude {
			if let Some((prev_range, _)) = prev {
				if prev_range == &exclude {
					prev = prev_iter.next_back();
				}
			}

			if let Some((next_range, _)) = prev {
				if next_range == &exclude {
					next = next_iter.next();
				}
			}
		}

		let prev_size = if let Some((prev, _)) = prev {
			if prev.end_addr() > virt_zone.addr() {
				return None;
			}
			virt_zone.as_usize() - prev.end_usize()
		} else {
			if virt_zone.as_usize() < PAGE_SIZE {
				return None;
			}
			virt_zone.as_usize() - PAGE_SIZE
		};

		let next_size = if let Some((next, _)) = next {
			if virt_zone.end_addr() > next.addr() {
				return None;
			}
			next.as_usize() - virt_zone.end_usize()
		} else {
			if virt_zone.end_usize() > *MAX_MAP_ADDR {
				return None;
			}
			*MAX_MAP_ADDR - virt_zone.end_usize()
		};

		Some((prev_size, next_size))
	}

	pub unsafe fn map(&self, mut phys_zones: VirtLayout) -> Result<VirtRange, MemErr>
	{
		// TODO: choose better zones based off alignment so more big pages cna be used saving tlb cache space
		let size = phys_zones.size();

		if size == 0 {
			return Err(MemErr::InvlVirtMem("tryed to map page of size zero"));
		}

		let mut btree = self.virt_map.lock();

		let virt_zone = Self::find_range(&btree, size).ok_or(MemErr::InvlVirtMem(
			"not enough space in virtual memory space for allocation",
		))?;

		let iter = PageMappingIterator::new(&phys_zones, &virt_zone);
		self.map_internal(iter);
		phys_zones.sync_mem();

		btree.insert(virt_zone, phys_zones);

		Ok(virt_zone)
	}

	pub unsafe fn map_at(
		&self,
		mut phys_zones: VirtLayout,
		virt_zone: VirtRange,
	) -> Result<VirtRange, MemErr>
	{
		let virt_zone = virt_zone.aligned();

		if phys_zones.size() != virt_zone.size() {
			return Err(MemErr::InvlArgs(
				"phys_zones and virt_zone size did not match",
			));
		}

		if phys_zones.size() == 0 {
			return Err(MemErr::InvlArgs("tryed to map page of size zero"));
		}

		// free_space already checks these, this is just for more accurate error message
		/*if virt_zone.as_usize () == 0
		{
			return Err(Err::new ("tried to map the null page"));
		}

		if virt_zone.end_usize () > *MAX_MAP_ADDR
		{
			return Err(Err::new ("attempted to map an address in the higher half kernel zone"));
		}*/

		let mut btree = self.virt_map.lock();

		if Self::free_space(&btree, virt_zone, None).is_none() {
			return Err(MemErr::InvlVirtMem("invalid virt zone passed to map_at"));
		}

		let iter = PageMappingIterator::new(&phys_zones, &virt_zone);
		self.map_internal(iter);
		phys_zones.sync_mem();

		btree.insert(virt_zone, phys_zones);

		Ok(virt_zone)
	}

	pub unsafe fn remap<F>(
		&self,
		virt_zone: VirtRange,
		atype: AllocType,
		alloc_func: F,
	) -> Result<VirtRange, MemErr>
	where
		F: FnOnce(&mut VirtLayout) -> Result<(), MemErr>,
	{
		let virt_zone = virt_zone.aligned();

		let mut btree = self.virt_map.lock();

		let virt_layout = btree
			.get_mut(&virt_zone)
			.ok_or(MemErr::InvlPtr("invalid virt zone passed to remap"))?;

		if virt_layout.alloc_type() != atype {
			return Err(MemErr::InvlMemType(
				"memory type does not match passed atype",
			));
		}

		alloc_func(virt_layout)?;

		let new_size = virt_layout.size();
		let nrange = VirtRange::new(virt_zone.addr(), new_size);

		if new_size == 0 {
			let mut phys_zones = btree.remove(&virt_zone).unwrap();

			let iter = PageMappingIterator::new(&phys_zones, &nrange);
			self.map_internal(iter);
			phys_zones.sync_mem();

			return Ok(VirtRange::null());
		}

		if Self::free_space(&btree, nrange, Some(virt_zone)).is_some() {
			let mut phys_zones = btree.remove(&virt_zone).unwrap();

			let iter = PageMappingIterator::new(&phys_zones, &nrange);
			self.map_internal(iter);
			phys_zones.sync_mem();

			btree.insert(nrange, phys_zones);

			Ok(nrange)
		} else {
			let new_virt_zone = Self::find_range(&btree, new_size).ok_or_else(|| {
				// make borrow checker happy
				let virt_layout = btree.get_mut(&virt_zone).unwrap();
				virt_layout.revert();
				MemErr::InvlVirtMem("not enough space in virtual memory space for allocation")
			})?;

			let mut phys_zones = btree.remove(&virt_zone).unwrap();

			let iter = PageMappingIterator::new_unmapper(&phys_zones, &virt_zone);
			self.map_internal(iter);
			phys_zones.mark_unmapped();

			let iter = PageMappingIterator::new(&phys_zones, &new_virt_zone);
			self.map_internal(iter);
			phys_zones.sync_mem();

			btree.insert(new_virt_zone, phys_zones);

			Ok(new_virt_zone)
		}
	}

	pub unsafe fn remap_at<F>(
		&self,
		virt_zone: VirtRange,
		target_addr: VirtAddr,
		atype: AllocType,
		alloc_func: F,
	) -> Result<VirtRange, MemErr>
	where
		F: FnOnce(&mut VirtLayout) -> Result<(), MemErr>,
	{
		let virt_zone = virt_zone.aligned();
		let target_addr = target_addr.align_down(PAGE_SIZE as u64);

		let mut btree = self.virt_map.lock();

		let virt_layout = btree
			.get_mut(&virt_zone)
			.ok_or(MemErr::InvlPtr("invalid virt zone passed to remap"))?;

		if virt_layout.alloc_type() != atype {
			return Err(MemErr::InvlMemType(
				"memory type does not match passed atype",
			));
		}

		alloc_func(virt_layout)?;

		let new_size = virt_layout.size();
		let nrange = VirtRange::new(target_addr, new_size);

		if new_size == 0 {
			let mut phys_zones = btree.remove(&virt_zone).unwrap();

			let iter = PageMappingIterator::new(&phys_zones, &nrange);
			self.map_internal(iter);
			phys_zones.sync_mem();

			return Ok(VirtRange::null());
		}

		if Self::free_space(&btree, nrange, Some(virt_zone)).is_some() {
			let mut phys_zones = btree.remove(&virt_zone).unwrap();

			if target_addr != virt_zone.addr() {
				let iter = PageMappingIterator::new_unmapper(&phys_zones, &virt_zone);
				self.map_internal(iter);
				phys_zones.mark_unmapped();
			}

			let iter = PageMappingIterator::new(&phys_zones, &nrange);
			self.map_internal(iter);
			phys_zones.sync_mem();

			btree.insert(nrange, phys_zones);

			Ok(nrange)
		} else {
			// make borrow checker happy
			let virt_layout = btree.get_mut(&virt_zone).unwrap();
			virt_layout.revert();
			Err(MemErr::InvlVirtMem(
				"could not remap virt zone to target virt zone",
			))
		}
	}

	pub unsafe fn unmap(&self, virt_zone: VirtRange, atype: AllocType)
		-> Result<VirtLayout, MemErr>
	{
		let mut btree = self.virt_map.lock();

		let virt_layout = btree
			.get_mut(&virt_zone)
			.ok_or(MemErr::InvlPtr("invalid virt zone passed to remap"))?;

		if virt_layout.alloc_type() != atype {
			return Err(MemErr::InvlMemType(
				"memory type does not match passed atype",
			));
		}

		// ok to unwrap because it is checked earlier
		let mut phys_zones = btree.remove(&virt_zone).unwrap();

		let iter = PageMappingIterator::new_unmapper(&phys_zones, &virt_zone);
		self.map_internal(iter);
		phys_zones.mark_unmapped();

		Ok(phys_zones)
	}

	// TODO: improve performance by caching previous virt parents
	unsafe fn map_internal(&self, iter: PageMappingIterator)
	{
		let cr3 = self.cr3.lock().as_mut().unwrap();

		for action in iter {
			let vframe = action.virt_frame();

			let addr = vframe.start_addr().as_u64() as usize;
			let nums = [
				get_bits(addr, 39..48),
				get_bits(addr, 30..39),
				get_bits(addr, 21..30),
				get_bits(addr, 12..21),
			];

			let (depth, hf) = match vframe {
				VirtFrame::K4(_) => (4, PageTableFlags::NONE),
				VirtFrame::M2(_) => (3, PageTableFlags::HUGE),
				VirtFrame::G1(_) => (2, PageTableFlags::HUGE),
			};

			match action {
				PageMappingAction::Map(pframe, _vframe, flags) => {
					let mut ptable = &mut *cr3;

					for d in 0..depth {
						let i = nums[d];
						if d == depth - 1 {
							let flags = flags | PageTableFlags::PRESENT | hf;
							ptable.set(i, PageTablePointer::new(pframe.start_addr(), flags));
						} else {
							ptable = ptable.get_or_alloc(i, self.frame_allocer, *PARENT_FLAGS);
						}
					}
				},
				PageMappingAction::Unmap(_vframe) => {
					let mut tables = [Some(&mut *cr3), None, None, None];

					for a in 1..depth {
						tables[a] = Some(tables[a - 1].as_mut().unwrap().get(nums[a - 1]));
					}

					for a in (0..depth).rev() {
						if !tables[a]
							.as_mut()
							.unwrap()
							.remove(nums[a], self.frame_allocer)
						{
							break;
						}
					}
				},
			}

			// TODO: check if address space is loaded before updating tlb cache
			invlpg(addr);
		}
	}
}

impl<T: FrameAllocator> Drop for VirtMapper<T>
{
	// Note: it is unsafe to drop a VirtMapper if the VirtMapper is currently loaded
	fn drop(&mut self)
	{
		unsafe {
			self.cr3
				.lock()
				.as_mut()
				.unwrap()
				.dealloc_all(self.frame_allocer);
		}
	}
}
