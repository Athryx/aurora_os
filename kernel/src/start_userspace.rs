use core::mem::size_of;

use bytemuck::{Pod, Zeroable, from_bytes, cast_slice, bytes_of};
use sys::{CapFlags, CapId, InitInfo};
use elf::{ElfBytes, endian::NativeEndian, abi::{PT_LOAD, PF_R, PF_W, PF_X}};
use aser::to_bytes_count_cap;

use crate::{prelude::*, alloc::{root_alloc, HeapRef, PaRef, root_alloc_page_ref, root_alloc_ref}, cap::{Capability, StrongCapability, memory::Memory}, process::{Spawner, PageMappingFlags, ThreadStartMode}};
use crate::process::Process;
use crate::container::Arc;

const INITRD_MAGIC: u64 = 0x39f298aa4b92e836;
const EARLY_INIT_ENTRY_TYPE: u64 = 1;

// hardcode these addressess to things which won't conflict
const STACK_ADDRESS: usize = 0x100000000;
const STACK_SIZE: usize = PAGE_SIZE * 8;
const STARTUP_DATA_ADDRESS: usize = 0x200000000;
const INITRD_MAPPING_ADDRESS: usize = 0x300000000;

const EARLY_INIT_ASLR_SEED: [u8; 32] =
    [12, 64, 89, 134, 11, 255, 123, 98, 12, 31, 2, 90, 38, 234, 3, 49, 32, 58, 238, 220, 1, 0, 24, 23, 9, 48, 28, 65, 1, 43, 54, 55];

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct InitRdHeader {
    magic: u64,
    len: u64,
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct InitRdProgram {
    program_type: u64,
    name: u64,
    name_size: u64,
    data: u64,
    data_size: u64,
}

/// Looks through the initrd and returns a slice to the elf binary data
fn find_early_init_data(initrd: &[u8]) -> &[u8] {
    let header: &InitRdHeader = from_bytes(&initrd[0..size_of::<InitRdHeader>()]);

    if header.magic != INITRD_MAGIC {
        panic!("invalid magic number on initrd");
    }

    let initrd_entry_bytes = &initrd[size_of::<InitRdHeader>()..];
    let initrd_entry_bytes = &initrd_entry_bytes[..header.len as usize * size_of::<InitRdProgram>()];
    let initrd_entries: &[InitRdProgram] = cast_slice(initrd_entry_bytes);

    for entry in initrd_entries {
        if entry.program_type == EARLY_INIT_ENTRY_TYPE {
            let start_index = entry.data as usize;
            let end_index = (entry.data + entry.data_size) as usize;
            return &initrd[start_index..end_index];
        }
    }

    panic!("could not find early init program in initrd");
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct MappedRegion {
    address: u64,
    size: u64,
    memory_id: u64,
    padding_start: u64,
    padding_end: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct StackInfo {
    process_data_address: usize,
    process_data_size: usize,
    startup_data_address: usize,
    startup_data_size: usize,
}

/// Parses the initrd and creates the early init process, which is the first userspace process
/// 
/// This code is not very robust for handling errors, but it doesn't need to be since if error occurs os will need to panic anyways
pub fn start_early_init_process(initrd: &[u8]) -> KResult<()> {
    // create process, and insert the needed capabilities
    let process_weak = Process::new(
        root_alloc_page_ref(),
        root_alloc_ref(),
        String::from_str(HeapRef::heap(), "early_init")?,
    )?;

    let spawner = Spawner::new(root_alloc_ref());
    spawner.add_process(process_weak.inner().clone())?;

    let process = process_weak.inner().upgrade().unwrap();

    let process_capability = Capability::Weak(process_weak);
    let spawner_capability = Capability::Strong(StrongCapability::new_flags(
        Arc::new(spawner, root_alloc_ref())?,
        CapFlags::READ | CapFlags::WRITE | CapFlags::PROD,
    ));
    let allocator_capability = Capability::Strong(StrongCapability::new_flags(
        root_alloc().clone(),
        CapFlags::READ | CapFlags::WRITE | CapFlags::PROD,
    ));

    let process_id = process.cap_map().insert_process(process_capability)?;
    let spawner_id = process.cap_map().insert_spawner(spawner_capability)?;
    let allocator_id = process.cap_map().insert_allocator(allocator_capability)?;

    let mut startup_data = Vec::new(root_alloc_ref());
    startup_data.extend_from_slice(&usize::from(process_id).to_le_bytes())?;
    startup_data.extend_from_slice(&usize::from(allocator_id).to_le_bytes())?;
    startup_data.extend_from_slice(&usize::from(spawner_id).to_le_bytes())?;

    // seed for aslr, we don't have rng at this point so it can't be random
    startup_data.extend_from_slice(&EARLY_INIT_ASLR_SEED)?;


    // maps memomry in the userspace process and adds it to the mapped regions list
    let mut map_memory = |address, size, flags| -> KResult<Arc<Memory>> {
        assert!(page_aligned(address));
        assert!(page_aligned(size));

        let memory = Arc::new(Memory::new(
            root_alloc_page_ref(),
            root_alloc_ref(),
            size / PAGE_SIZE,
        )?, root_alloc_ref())?;

        let memory_capability = StrongCapability::new_flags(
            memory.clone(),
            CapFlags::READ | CapFlags::WRITE | CapFlags::PROD,
        );

        let memory_id = process.cap_map().insert_memory(Capability::Strong(memory_capability))?;
        let Capability::Strong(memory_capability) = process.cap_map().get_memory(memory_id)? else {
            panic!("invalid capability returned")
        };

        process.map_memory(
            memory_capability,
            VirtAddr::new(address),
            Some(size / PAGE_SIZE),
            flags,
        )?;

        let region = MappedRegion {
            address: address as u64,
            size: size as u64,
            memory_id: usize::from(memory_id) as u64,
            padding_start: 0,
            padding_end: 0,
        };

        startup_data.extend_from_slice(bytes_of(&region))?;

        Ok(memory)
    };

    // parse elf data and map memory regions
    let early_init_bytes = find_early_init_data(initrd);
    let elf_data = ElfBytes::<NativeEndian>::minimal_parse(early_init_bytes).unwrap();

    for phdr in elf_data.segments().unwrap().iter() {
        if phdr.p_type == PT_LOAD {
            let mut map_flags = PageMappingFlags::USER;
            if phdr.p_flags & PF_R != 0 {
                map_flags |= PageMappingFlags::READ;
            }
            if phdr.p_flags & PF_W != 0 {
                map_flags |= PageMappingFlags::WRITE;
            }
            if phdr.p_flags & PF_X != 0 {
                map_flags |= PageMappingFlags::EXEC;
            }

            // it seems elf doesn't require address or size to be page aligned
            let unaligned_map_range = UVirtRange::new(
                VirtAddr::new(phdr.p_vaddr as usize),
                phdr.p_memsz as usize,
            );
            let map_range = unaligned_map_range.as_aligned();

            let memory = map_memory(map_range.as_usize(), map_range.size(), map_flags)
                .expect("mapping memory failed");


            let section_data = elf_data.segment_data(&phdr).unwrap();

            let mut memory_inner = memory.inner();
            let mut allocation = memory_inner.get_allocation_mut(0).unwrap();

            let mem_slice = allocation.as_mut_slice();
            mem_slice.fill(0);

            let start_index = phdr.p_vaddr as usize - map_range.as_usize();
            let end_index = start_index + section_data.len();
            mem_slice[start_index..end_index].copy_from_slice(section_data);
        }
    }

    let stack_memory = map_memory(
        STACK_ADDRESS,
        STACK_SIZE,
        PageMappingFlags::USER | PageMappingFlags::READ | PageMappingFlags::WRITE,
    ).expect("mapping stack failed");

    let startup_data_memory = map_memory(
        STARTUP_DATA_ADDRESS,
        PAGE_SIZE,
        PageMappingFlags::USER | PageMappingFlags::READ | PageMappingFlags::WRITE,
    ).expect("mapping startup data failed");

    let initrd_memory = map_memory(
        INITRD_MAPPING_ADDRESS,
        align_up(initrd.len(), PAGE_SIZE),
        PageMappingFlags::USER | PageMappingFlags::READ | PageMappingFlags::WRITE,
    ).expect("failed to map initrd memory");

    initrd_memory.inner()
        .get_allocation_mut(0)
        .unwrap()
        .copy_from_mem(initrd);


    // append init info to startup data
    let init_info = InitInfo {
        initrd_address: INITRD_MAPPING_ADDRESS,
    };

    let init_bytes: Vec<u8> = to_bytes_count_cap(&init_info)
        .expect("faield to serialize init info");

    let process_data_address = STARTUP_DATA_ADDRESS;
    let process_data_size = startup_data.len();
    let startup_data_address = process_data_address + process_data_size;

    startup_data.extend_from_slice(&init_bytes)?;

    let startup_data_size = startup_data.len() - process_data_size;


    // write startup data to startup data meomry
    startup_data_memory.inner()
        .get_allocation_mut(0)
        .unwrap()
        .copy_from_mem(&startup_data);


    // write pointers to stack
    let stack_info = StackInfo {
        process_data_address,
        process_data_size,
        startup_data_address,
        startup_data_size,
    };

    let stack_info_address = stack_memory.inner()
        .get_allocation_mut(0)
        .unwrap()
        .as_vrange()
        .end_addr() - size_of::<StackInfo>();

    unsafe {
        ptr::write(stack_info_address.as_mut_ptr(), stack_info);
    }


    // start the first thread
    let rip = elf_data.ehdr.e_entry as usize;
    let rsp = STACK_ADDRESS + STACK_SIZE - size_of::<StackInfo>();
    let thread_name = String::from_str(root_alloc_ref(), "early_init_thread")?;
    eprintln!("starting first userspace process: {}", process.name());
    eprintln!("rip: 0x{:x}", rip);
    eprintln!("rsp: 0x{:x}", rsp);

    process.create_thread(
        thread_name,
        ThreadStartMode::Ready,
        rip,
        rsp,
    ).expect("failed to create thread");

    Ok(())
}