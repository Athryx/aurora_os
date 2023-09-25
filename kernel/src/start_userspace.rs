use core::mem::size_of;

use bytemuck::{Pod, Zeroable, from_bytes, cast_slice, bytes_of};
use sys::{CapFlags, InitInfo, ProcessInitData, ProcessMemoryEntry, StackInfo};
use elf::{ElfBytes, endian::NativeEndian, abi::{PT_LOAD, PF_R, PF_W, PF_X}};
use aser::to_bytes_count_cap;

use crate::{prelude::*, alloc::{root_alloc, root_alloc_page_ref, root_alloc_ref}, cap::{Capability, StrongCapability, memory::Memory, address_space::AddressSpace, capability_space::CapabilitySpace, WeakCapability}, sched::{ThreadGroup, Thread, ThreadStartMode}};
use crate::vmem_manager::PageMappingFlags;
use crate::container::Arc;

const INITRD_MAGIC: u64 = 0x39f298aa4b92e836;
const EARLY_INIT_ENTRY_TYPE: u64 = 1;

// hardcode these addressess to things which won't conflict
const STACK_ADDRESS: usize = 0x100000000;
const STACK_SIZE: Size = Size::from_pages(16);
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

/// Parses the initrd and creates the early init process, which is the first userspace process
/// 
/// This code is not very robust for handling errors, but it doesn't need to be since if error occurs os will need to panic anyways
pub fn start_early_init_process(initrd: &[u8]) -> KResult<()> {
    // create first process context, and insert needed capabilities
    let thread_group = Arc::new(
        ThreadGroup::new(root_alloc_page_ref(), root_alloc_ref()),
        root_alloc_ref(),
    )?;
    let thread_group_capability = Capability::Strong(StrongCapability::new_flags(
        thread_group.clone(),
        CapFlags::all(),
    ));

    let address_space = Arc::new(
        AddressSpace::new(root_alloc_page_ref(), root_alloc_ref())?,
        root_alloc_ref(),
    )?;
    let address_space_capability = Capability::Strong(StrongCapability::new_flags(
        address_space.clone(),
        CapFlags::all(),
    ));

    let capability_space = Arc::new(
        CapabilitySpace::new(root_alloc_ref()),
        root_alloc_ref(),
    )?;
    let cspace_capability = Capability::Weak(WeakCapability::new_flags(
        Arc::downgrade(&capability_space),
        CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
    ));

    let allocator_capability = Capability::Strong(StrongCapability::new_flags(
        root_alloc().clone(),
        CapFlags::all(),
    ));

    
    // list of memory regions that have been mapped
    let mut memory_regions = Vec::new(root_alloc_ref());

    // maps memomry in the userspace process and adds it to the mapped regions list
    let mut map_memory = |address, size: Size, flags| -> KResult<Arc<Memory>> {
        assert!(page_aligned(address));
        assert!(size.is_page_aligned());

        let memory = Arc::new(Memory::new(
            root_alloc_page_ref(),
            root_alloc_ref(),
            size.pages_rounded(),
        )?, root_alloc_ref())?;

        let memory_capability = StrongCapability::new_flags(
            memory.clone(),
            CapFlags::all(),
        );

        let memory_id = capability_space.insert_memory(Capability::Strong(memory_capability))?;

        address_space.map_memory(
            memory.clone(),
            VirtAddr::new(address),
            Some(size),
            flags,
        )?;

        let region = ProcessMemoryEntry {
            memory_cap_id: memory_id.into(),
            memory_size: memory.inner_read().size().bytes(),
            map_address: address,
            map_size: size.bytes(),
            padding_start: 0,
            padding_end: 0,
        };

        memory_regions.push(region)?;

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

            let memory = map_memory(
                map_range.as_usize(),
                Size::from_bytes(map_range.size()),
                map_flags,
            ).expect("mapping memory failed");


            let section_data = elf_data.segment_data(&phdr).unwrap();

            let memory_inner = memory.inner_read();

            unsafe {
                memory_inner.zero();

                let write_offset = phdr.p_vaddr as usize - map_range.as_usize();
                memory_inner.copy_from(write_offset.., section_data);
            }
        }
    }

    let stack_memory = map_memory(
        STACK_ADDRESS,
        STACK_SIZE,
        PageMappingFlags::USER | PageMappingFlags::READ | PageMappingFlags::WRITE,
    ).expect("mapping stack failed");

    let startup_data_memory = map_memory(
        STARTUP_DATA_ADDRESS,
        Size::from_pages(1),
        PageMappingFlags::USER | PageMappingFlags::READ | PageMappingFlags::WRITE,
    ).expect("mapping startup data failed");

    let initrd_memory = map_memory(
        INITRD_MAPPING_ADDRESS,
        Size::from_bytes(align_up(initrd.len(), PAGE_SIZE)),
        PageMappingFlags::USER | PageMappingFlags::READ | PageMappingFlags::WRITE,
    ).expect("failed to map initrd memory");

    unsafe {
        initrd_memory.inner_read().copy_from(.., initrd);
    }


    // create first thread
    let rip = elf_data.ehdr.e_entry as usize;
    let rsp = STACK_ADDRESS + STACK_SIZE.bytes() - size_of::<StackInfo>();
    let thread_name = String::from_str(root_alloc_ref(), "early_init_thread")?;
    let thread = ThreadGroup::create_thread(
        &thread_group,
        address_space,
        capability_space.clone(),
        thread_name,
        ThreadStartMode::Suspended,
        rip,
        rsp,
    )?;
    let thread_capability = Capability::Weak(WeakCapability::new_flags(
        Arc::downgrade(&thread),
        CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
    ));


    // add all capabilities to capability space
    let thread_group_id = capability_space.insert_thread_group(thread_group_capability)?.into();
    let address_space_id = capability_space.insert_address_space(address_space_capability)?.into();
    let capability_space_id = capability_space.insert_capability_space(cspace_capability)?.into();
    let allocator_id = capability_space.insert_allocator(allocator_capability)?.into();
    let thread_id = capability_space.insert_thread(thread_capability)?.into();
    let process_init_data = ProcessInitData {
        thread_group_id,
        address_space_id,
        capability_space_id,
        allocator_id,
        main_thread_id: thread_id,
        stack_region_start_address: STACK_ADDRESS,
        aslr_seed: EARLY_INIT_ASLR_SEED,
    };


    // create startup data for early-init
    let mut startup_data = Vec::new(root_alloc_ref());
    startup_data.extend_from_slice(bytes_of(&process_init_data))?;
    startup_data.extend_from_slice(cast_slice(&memory_regions))?;


    // append init info to startup data
    let init_info = InitInfo {
        initrd_address: INITRD_MAPPING_ADDRESS,
    };

    let namespace_data: Vec<u8> = to_bytes_count_cap(&init_info)
        .expect("faield to serialize init info");

    let process_data_address = STARTUP_DATA_ADDRESS;
    let process_data_size = startup_data.len();
    let namespace_data_address = process_data_address + process_data_size;
    let namespace_data_size = namespace_data.len();

    startup_data.extend_from_slice(&namespace_data)?;


    // write startup data to startup data memory
    unsafe {
        startup_data_memory.inner_read().copy_from(.., startup_data.as_slice());
    }


    // write pointers to stack
    let stack_info = StackInfo {
        process_data_address,
        process_data_size,
        namespace_data_address,
        namespace_data_size,
    };

    unsafe {
        let stack_memory_inner = stack_memory.inner_read();
        let stack_memory_size = stack_memory_inner.size().bytes();
        stack_memory.inner_read().copy_from(stack_memory_size - size_of::<StackInfo>().., bytes_of(&stack_info));
    }


    // start the first thread
    eprintln!("starting first userspace process");
    eprintln!("rip: 0x{:x}", rip);
    eprintln!("rsp: 0x{:x}", rsp);
    Thread::resume_suspended_thread(&thread)
        .expect("failed to resume first thread");

    Ok(())
}