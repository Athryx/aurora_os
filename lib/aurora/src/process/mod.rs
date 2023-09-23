use core::mem::size_of;

use crate::allocator::addr_space::{RemoteAddrSpaceManager, AddrSpaceError, MapMemoryArgs, RegionPadding};
use crate::env::{Args, Namespace};

use aser::{Value, to_bytes_count_cap, AserError, AserCloneCapsError};
use bit_utils::{align_down, PAGE_SIZE, align_up, Size};
use elf::abi::{PT_LOAD, PF_R, PF_W, PF_X};
use elf::{ElfBytes, ParseError};
use elf::endian::NativeEndian;
use serde::Serialize;
use sys::{CapFlags, SysErr, Thread, AddressSpace, ThreadStartMode, MemoryMappingFlags, ProcessInitData, ProcessMemoryEntry, cap_clone, CspaceTarget, Capability, StackInfo};
use thiserror_no_std::Error;
use bytemuck::bytes_of;

use crate::{prelude::*, this_context};
use crate::collections::HashMap;

const DEFAULT_STACK_SIZE: Size = Size::from_pages(64);
const DEFAULT_STACK_PADDING: Size = Size::from_pages(1024);

/// Terminates the current process
pub fn exit() -> ! {
    let _ = this_context().thread_group.exit();

    loop { core::hint::spin_loop(); }
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("System error: {0}")]
    SysErr(#[from] SysErr),
    #[error("Error parsing elf data: {0}")]
    ElfParseError(#[from] ParseError),
    #[error("The supplied elf file did not contain any elf segments")]
    NoElfSegments,
    #[error("The elf segment was bigger than the specified memsz")]
    ElfSegmentToBig,
    #[error("Error mapping memory in new process: {0}")]
    AddrSpaceError(#[from] AddrSpaceError),
    #[error("Failed to serialize new process namespace: {0}")]
    SerializetionError(#[from] AserError),
    #[error("Failed to transfer capabilities in namespace to new process: {0}")]
    TransferCapError(#[from] AserCloneCapsError),
}

/// Where the elf data to launc hthe process is comming from
enum ProcessDataSource {
    Bytes(Vec<u8>),
}

impl ProcessDataSource {
    fn bytes(&mut self) -> &[u8] {
        match self {
            Self::Bytes(data) => data,
        }
    }
}

#[derive(Default)]
struct ArgsBuilder {
    positional_args: Vec<Value>,
    named_args: HashMap<String, Value>,
}

impl From<&ArgsBuilder> for Args {
    fn from(value: &ArgsBuilder) -> Self {
        Args {
            positional_args: value.positional_args.clone(),
            named_args: Value::from_serialize(&value.named_args)
                .expect("failed to build arguments for new process"),
        }
    }
}

/// Used to execute other processess
/// 
/// Functions similarly to the standard library's Command
pub struct Command {
    process_data: ProcessDataSource,
    args: ArgsBuilder,
}

impl Command {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Command {
            process_data: ProcessDataSource::Bytes(bytes),
            args: ArgsBuilder::default(),
        }
    }

    pub fn arg<T: Serialize>(&mut self, arg: &T) -> &mut Self {
        self.args.positional_args.push(
            Value::from_serialize(arg).expect("failed to serialize process argument"),
        );
        self
    }

    pub fn args<T: Serialize, I: IntoIterator<Item = T>>(&mut self, args: I) -> &mut Self {
        for arg in args {
            self.arg(&arg);
        }

        self
    }

    pub fn named_arg<T: Serialize>(&mut self, arg_name: String, arg: &T) -> &mut Self {
        let arg_value = Value::from_serialize(arg)
            .expect("failed to serialize process argument");

        self.args.named_args.insert(arg_name, arg_value);

        self
    }

    pub fn spawn(&mut self) -> Result<Child, ProcessError> {
        let namespace = Namespace {
            args: Args::from(&self.args),
        };

        let exe_data = self.process_data.bytes();

        spawn_process(exe_data, &namespace)
    }
}

pub struct Child {}

fn spawn_process(exe_data: &[u8], namespace: &Namespace) -> Result<Child, ProcessError> {
    let aslr_seed = gen_aslr_seed();

    let allocator = &this_context().allocator;

    let thread_group = this_context().thread_group.new_child_group(allocator)?;
    let address_space = AddressSpace::new(allocator)?;

    let mut manager = RemoteAddrSpaceManager::new_remote(aslr_seed, allocator, &address_space)?;

    let elf_data = ElfBytes::<NativeEndian>::minimal_parse(exe_data)?;
    let rip = elf_data.ehdr.e_entry as usize;

    for phdr in elf_data.segments().ok_or(ProcessError::NoElfSegments)?.iter() {
        if phdr.p_type == PT_LOAD {
            let map_flags = elf_flags_to_memory_mapping_flags(phdr.p_flags);

            let start_addr = phdr.p_vaddr as usize;
            let end_addr = start_addr + phdr.p_memsz as usize;

            // elf does not require page aligned addressess
            let aligned_start_addr = align_down(start_addr, PAGE_SIZE);
            let aligned_end_addr = align_up(end_addr, PAGE_SIZE);
            let map_size = aligned_end_addr - aligned_start_addr;
            if map_size == 0 {
                continue;
            }

            let section_mapping = manager.map_memory_remote_and_local(MapMemoryArgs {
                address: Some(aligned_start_addr),
                size: Some(Size::from_bytes(map_size)),
                flags: map_flags,
                ..Default::default()
            })?;

            let section_data = elf_data.segment_data(&phdr)?;
            if section_data.len() > phdr.p_memsz as usize {
                return Err(ProcessError::ElfSegmentToBig);
            }

            // offset from start of mapping where elf section data should be placed
            let offset = phdr.p_vaddr as usize - aligned_start_addr;
            if section_data.len() + offset > section_mapping.size.bytes() {
                return Err(ProcessError::ElfSegmentToBig);
            }

            let dest_addr = section_mapping.local_address.unwrap() + offset;
            let dest_ptr = dest_addr as *mut u8;

            unsafe {
                core::ptr::copy_nonoverlapping(section_data.as_ptr(), dest_ptr, section_data.len());
            }

            let padding_ptr = (dest_addr + section_data.len()) as *mut u8;
            // this will not overflow since it is already checked that memsz >= section data len
            let pading_size = phdr.p_memsz as usize - section_data.len();

            unsafe {
                core::ptr::write_bytes(padding_ptr, 0, pading_size);
            }
        }
    }


    // map stack in this process and new process
    let stack = manager.map_memory_remote_and_local(MapMemoryArgs {
        size: Some(DEFAULT_STACK_SIZE),
        flags: MemoryMappingFlags::READ | MemoryMappingFlags::WRITE,
        padding: RegionPadding {
            start: DEFAULT_STACK_PADDING,
            ..Default::default()
        },
        ..Default::default()
    })?;
    let rsp = stack.remote_address + stack.size.bytes() - size_of::<StackInfo>();


    let mut namespace_data: Vec<u8> = to_bytes_count_cap(&namespace)?;

    let startup_data_size = calc_process_startup_data_size(
        &manager,
        namespace_data.len()
    );

    // map startup data memory in new process and current process
    let startup_data_mapping = manager.map_memory_remote_and_local(MapMemoryArgs {
        size: Some(startup_data_size),
        flags: MemoryMappingFlags::READ,
        ..Default::default()
    })?;


    let (thread, cspace) = Thread::new_with_cspace(
        allocator,
        &thread_group,
        &address_space,
        rip,
        rsp,
        ThreadStartMode::Suspended,
    )?;

    // move necessary capabilitys to new process cspace
    let dst_cspace = CspaceTarget::Other(&cspace);
    let thread_group_id = cap_clone(dst_cspace, CspaceTarget::Current, &thread_group, CapFlags::all())?
        .into_cap_id()
        .into();
    let address_space_id = cap_clone(dst_cspace, CspaceTarget::Current, &address_space, CapFlags::all())?
        .into_cap_id()
        .into();
    let capability_space_id = cap_clone(dst_cspace, CspaceTarget::Current, &cspace, CapFlags::all())?
        .into_cap_id()
        .into();
    let allocator_id = cap_clone(dst_cspace, CspaceTarget::Current, allocator, CapFlags::all())?
        .into_cap_id()
        .into();
    let main_thread_id = cap_clone(dst_cspace, CspaceTarget::Current, &thread, CapFlags::all())?
        .into_cap_id()
        .into();
    aser::clone_caps_to_cspace(dst_cspace, &mut namespace_data)?;

    let process_init_data = ProcessInitData {
        thread_group_id,
        address_space_id,
        capability_space_id,
        allocator_id,
        main_thread_id,
        stack_region_start_address: stack.remote_address,
        aslr_seed,
    };

    // create startup data bytes
    let mut startup_data = Vec::new();
    startup_data.extend_from_slice(bytes_of(&process_init_data));

    for mapping in manager.memory_regions.iter() {
        // we don't care about communicating reserved memory regions to new process
        if let Some(memory) = &mapping.memory_cap {
            let memory_id = cap_clone(dst_cspace, CspaceTarget::Current, memory, CapFlags::all())?
                .into_cap_id()
                .into();

            let memory_entry = ProcessMemoryEntry {
                memory_cap_id: memory_id,
                map_address: mapping.address,
                map_size: mapping.size.bytes(),
                padding_start: mapping.padding.start.bytes(),
                padding_end: mapping.padding.end.bytes(),
            };

            startup_data.extend_from_slice(bytes_of(&memory_entry));
        }
    }

    let init_data_len = startup_data.len();
    startup_data.extend_from_slice(&namespace_data);


    // write startup data to memory in new process
    unsafe {
        core::ptr::copy_nonoverlapping(
            startup_data.as_ptr(),
            startup_data_mapping.local_address.unwrap() as *mut u8,
            startup_data.len(),
        );
    }


    // put pointers to startup data on new stack
    let stack_info = StackInfo {
        process_data_address: startup_data_mapping.remote_address,
        process_data_size: init_data_len,
        namespace_data_address: startup_data_mapping.remote_address + init_data_len,
        namespace_data_size: namespace_data.len(),
    };

    let local_rsp = stack.local_address.unwrap() + stack.size.bytes() - size_of::<StackInfo>();
    unsafe {
        core::ptr::write(local_rsp as *mut StackInfo, stack_info);
    }

    thread.resume()?;

    Ok(Child {})
}

fn gen_aslr_seed() -> [u8; 32] {
    // TODO: implement once randomness is a thing
    [12, 64, 89, 134, 11, 235, 123, 98, 12, 31, 2, 90, 38, 24, 3, 49, 32, 58, 238, 210, 1, 0, 24, 23, 9, 48, 28, 65, 1, 43, 54, 55]
}

fn elf_flags_to_memory_mapping_flags(elf_flags: u32) -> MemoryMappingFlags {
    let mut map_flags = MemoryMappingFlags::empty();
    if elf_flags & PF_R != 0 {
        map_flags |= MemoryMappingFlags::READ;
    }
    if elf_flags & PF_W != 0 {
        map_flags |= MemoryMappingFlags::WRITE;
    }
    if elf_flags & PF_X != 0 {
        map_flags |= MemoryMappingFlags::EXEC;
    }

    map_flags
}

/// Calculates the size of the memory we need to allocate to hold all the startup data
fn calc_process_startup_data_size(addr_space_manager: &RemoteAddrSpaceManager, namespace_data_len: usize) -> Size {
    let size = size_of::<ProcessInitData>()
        // + 1 for the memory we will have to allocate to hold startup data
        + (addr_space_manager.memory_regions.len() + 1) * size_of::<ProcessMemoryEntry>()
        + namespace_data_len;
    
    Size::from_bytes(size)
}