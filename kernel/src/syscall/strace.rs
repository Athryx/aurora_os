//! Gets a human readable view of each syscall invocation
// TODO: put this in a seperate library

use core::fmt::{self, Display, Write};

use sys::{CapId, syscall_nums::*, ThreadNewFlags, ThreadDestroyFlags, ThreadSuspendFlags, HandleEventSyncFlags, HandleEventAsyncFlags, CapCloneFlags, CapDestroyFlags, MemoryNewFlags, MemoryUpdateMappingFlags, MemoryResizeFlags, EventPoolAwaitFlags, ChannelSyncFlags, ChannelAsyncRecvFlags, MemoryMappingFlags};
use bitflags::Flags;

use crate::prelude::*;
use crate::mem::{HeapRef, root_alloc_ref};
use super::SyscallVals;

#[derive(Debug, Clone, Copy)]
pub enum Arg {
    Address(usize),
    CapId(Option<CapId>),
    Num(usize),
}

impl Display for Arg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(addr) => write!(f, "0x{:x}", addr),
            Self::CapId(None) => write!(f, "<invalid capid>"),
            Self::CapId(Some(cap_id)) => write!(f, "{}", cap_id),
            Self::Num(num) => write!(f, "{}", num),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ArgType {
    Address,
    CapId,
    Num,
}

struct StraceArgsBuilder {
    options: String,
    args: Vec<Arg>,
}

impl StraceArgsBuilder {
    pub fn new(allocator: HeapRef) -> Self {
        StraceArgsBuilder {
            options: String::new(allocator.clone()),
            args: Vec::new(allocator),
        }
    }

    // TODO: detect weak autodestroy
    pub fn options<T: Flags>(&mut self, flags: T) {
        for (i, (flag_name, _)) in flags.iter_names().enumerate() {
            if i != 0 {
                write!(self.options, " | ").unwrap();
            }

            write!(self.options, "{}", flag_name).unwrap();
        }
    }

    // alot of these can panic on oom, but panic safety is not very important for a debug feature only
    pub fn addr(&mut self, addr: usize) {
        self.args.push(Arg::Address(addr)).unwrap();
    }

    pub fn cap_id(&mut self, cap_id: usize) {
        self.args.push(Arg::CapId(CapId::try_from(cap_id))).unwrap();
    }

    pub fn num(&mut self, num: usize) {
        self.args.push(Arg::Num(num)).unwrap();
    }

    pub fn arg(&mut self, arg_type: ArgType, n: usize) {
        match arg_type {
            ArgType::Address => self.addr(n),
            ArgType::CapId => self.cap_id(n),
            ArgType::Num => self.num(n),
        }
    }
}

impl Display for StraceArgsBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.options)?;

        for arg in self.args.iter() {
            write!(f, ", {}", arg)?;
        }

        Ok(())
    }
}

macro_rules! argsf {
    ($vals:expr, $flag_ty:ty, $($args:ident,)*) => {{
        let mut args = StraceArgsBuilder::new(root_alloc_ref());
        let flags = <$flag_ty>::from_bits_truncate($vals.options);
        args.options(flags);

        let arg_types = [$(ArgType::$args,)*];
        for (i, arg_type) in arg_types.iter().enumerate() {
            args.arg(*arg_type, $vals.get(i).expect("too many args"));
        }

        args
    }};
}

macro_rules! args {
    ($vals:expr, $($args:ident,)*) => {{
        let mut args = StraceArgsBuilder::new(root_alloc_ref());

        let arg_types = [$(ArgType::$args,)*];
        for (i, arg_type) in arg_types.iter().enumerate() {
            args.arg(*arg_type, $vals.get(i).expect("too many args"));
        }

        args
    }};
}

macro_rules! event_sync {
    ($vals:tt) => {
        argsf!($vals, HandleEventSyncFlags, CapId, Num,)
    }
}

macro_rules! event_async {
    ($vals:tt) => {
        argsf!($vals, HandleEventAsyncFlags, CapId, CapId, Num,)
    }
}

pub fn get_strace_args_string(syscall_num: u32, vals: &SyscallVals) -> String {
	let syscall_name = String::from_str(root_alloc_ref(), syscall_name(syscall_num)).unwrap();

    let args = match syscall_num {
        PRINT_DEBUG => return syscall_name,
        THREAD_GROUP_NEW => args!(vals, CapId, CapId,),
        THREAD_GROUP_EXIT => args!(vals, CapId,),
        THREAD_NEW => argsf!(vals, ThreadNewFlags, CapId, CapId, CapId, CapId, Address, Address,),
        THREAD_YIELD => args!(vals,),
        THREAD_DESTROY => argsf!(vals, ThreadDestroyFlags, CapId,),
        THREAD_SUSPEND => argsf!(vals, ThreadSuspendFlags, Num,),
        THREAD_RESUME => args!(vals, CapId,),
        THREAD_SET_PROPERTY => args!(vals, Num, Address,),
        THREAD_HANDLE_THREAD_EXIT_SYNC => event_sync!(vals),
        THREAD_HANDLE_THREAD_EXIT_ASYNC => event_async!(vals),
        // TODO: fix flags
        CAP_CLONE => argsf!(vals, CapCloneFlags, CapId, CapId, CapId,),
        CAP_DESTROY => argsf!(vals, CapDestroyFlags, CapId, CapId,),
        ADDRESS_SPACE_NEW => args!(vals, CapId,),
        ADDRESS_SPACE_UNMAP => args!(vals, CapId, Address,),
        // TODO: include MemoryMapFlags options as well
        MEMORY_MAP => argsf!(vals, MemoryMappingFlags, CapId, CapId, Address, Num, Num,),
        MEMORY_UPDATE_MAPPING => argsf!(vals, MemoryUpdateMappingFlags, CapId, Address, Num,),
        MEMORY_NEW => argsf!(vals, MemoryNewFlags, CapId, Num,),
        MEMORY_GET_SIZE => args!(vals, CapId,),
        MEMORY_RESIZE => argsf!(vals, MemoryResizeFlags, CapId, Num,),
        EVENT_POOL_NEW => args!(vals, CapId, Num,),
        EVENT_POOL_MAP => args!(vals, CapId, CapId, Address,),
        EVENT_POOL_AWAIT => argsf!(vals, EventPoolAwaitFlags, CapId, Num,),
        // TODO: cap flags
        CHANNEL_NEW => args!(vals, CapId,),
        CHANNEL_TRY_SEND => args!(vals, CapId, CapId, Num, Num,),
        CHANNEL_SYNC_SEND => argsf!(vals, ChannelSyncFlags, CapId, CapId, Num, Num, Num,),
        CHANNEL_ASYNC_SEND => args!(vals, CapId, CapId, Num, Num, CapId, Num,),
        CHANNEL_TRY_RECV => args!(vals, CapId, CapId, Num, Num,),
        CHANNEL_SYNC_RECV => argsf!(vals, ChannelSyncFlags, CapId, CapId, Num, Num, Num,),
        CHANNEL_ASYNC_RECV => argsf!(vals, ChannelAsyncRecvFlags, CapId, CapId, Num,),
        CHANNEL_SYNC_CALL => argsf!(vals, ChannelSyncFlags, CapId, CapId, Num, Num, CapId, Num, Num, Num,),
        CHANNEL_ASYNC_CALL => args!(vals, CapId, CapId, Num, Num, CapId, Num,),
        REPLY_REPLY => args!(vals, CapId, CapId, Num, Num,),
        // TODO: cap flags
        KEY_NEW => args!(vals, CapId,),
        KEY_ID => args!(vals, CapId,),
        DROP_CHECK_NEW => args!(vals, CapId, Num,),
        DROP_CHECK_RECIEVER_HANDLE_CAP_DROP_SYNC => event_sync!(vals),
        DROP_CHECK_RECIEVER_HANDLE_CAP_DROP_ASYNC => event_async!(vals),
        MMIO_ALLOCATOR_ALLOC => args!(vals, CapId, CapId, Address, Num,),
        PHYS_MEM_MAP => argsf!(vals, MemoryMappingFlags, CapId, CapId, Address,),
        PHYS_MEM_GET_SIZE => args!(vals, CapId,),
        _ => return syscall_name,
    };

	format!(root_alloc_ref(), "sys {}({})", syscall_name, args)
}

struct StraceRetBuilder {
    args: Vec<Arg>,
}

impl StraceRetBuilder {
    pub fn new(allocator: HeapRef) -> Self {
        StraceRetBuilder {
            args: Vec::new(allocator),
        }
    }

    // alot of these can panic on oom, but panic safety is not very important for a debug feature only
    pub fn addr(&mut self, addr: usize) {
        self.args.push(Arg::Address(addr)).unwrap();
    }

    pub fn cap_id(&mut self, cap_id: usize) {
        self.args.push(Arg::CapId(CapId::try_from(cap_id))).unwrap();
    }

    pub fn num(&mut self, num: usize) {
        self.args.push(Arg::Num(num)).unwrap();
    }

    pub fn arg(&mut self, arg_type: ArgType, n: usize) {
        match arg_type {
            ArgType::Address => self.addr(n),
            ArgType::CapId => self.cap_id(n),
            ArgType::Num => self.num(n),
        }
    }
}

impl Display for StraceRetBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, arg) in self.args.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }

            write!(f, "{}", arg)?;
        }

        Ok(())
    }
}

macro_rules! ret {
    () => {
        StraceRetBuilder::new(root_alloc_ref())
    };
    ($vals:expr, $($ret:ident,)*) => {{
        let mut args = StraceRetBuilder::new(root_alloc_ref());

        let arg_types = [$(ArgType::$ret,)*];
        for (i, arg_type) in arg_types.iter().enumerate() {
            args.arg(*arg_type, $vals.get(i + 1).expect("too many args"));
        }

        args
    }};
}

pub fn get_strace_return_string(syscall_num: u32, vals: &SyscallVals) -> String {
    if vals.a1 == SysErr::Ok.num() {
        let mut out = String::from_str(root_alloc_ref(), "Ok(").unwrap();

        let ret_values = match syscall_num {
            PRINT_DEBUG => ret!(),
            THREAD_GROUP_NEW => ret!(vals, CapId,),
            THREAD_GROUP_EXIT => ret!(),
            THREAD_NEW => ret!(vals, CapId, CapId,),
            THREAD_YIELD => ret!(),
            THREAD_DESTROY => ret!(),
            THREAD_SUSPEND => ret!(),
            THREAD_RESUME => ret!(),
            THREAD_SET_PROPERTY => ret!(),
            THREAD_HANDLE_THREAD_EXIT_SYNC => ret!(),
            THREAD_HANDLE_THREAD_EXIT_ASYNC => ret!(),
            CAP_CLONE => ret!(vals, CapId,),
            CAP_DESTROY => ret!(),
            ADDRESS_SPACE_NEW => ret!(vals, CapId,),
            ADDRESS_SPACE_UNMAP => ret!(),
            MEMORY_MAP => ret!(vals, Num,),
            MEMORY_UPDATE_MAPPING => ret!(vals, Num,),
            MEMORY_NEW => ret!(vals, CapId, Num,),
            MEMORY_GET_SIZE => ret!(vals, Num,),
            MEMORY_RESIZE => ret!(vals, Num,),
            EVENT_POOL_NEW => ret!(vals, CapId,),
            EVENT_POOL_MAP => ret!(vals, Num,),
            EVENT_POOL_AWAIT => ret!(vals, Address, Num,),
            CHANNEL_NEW => ret!(vals, CapId,),
            CHANNEL_TRY_SEND => ret!(vals, Num,),
            CHANNEL_SYNC_SEND => ret!(vals, Num,),
            CHANNEL_ASYNC_SEND => ret!(),
            CHANNEL_TRY_RECV => ret!(vals, Num, CapId,),
            CHANNEL_SYNC_RECV => ret!(vals, Num, CapId,),
            CHANNEL_ASYNC_RECV => ret!(),
            CHANNEL_SYNC_CALL => ret!(vals, Num,),
            CHANNEL_ASYNC_CALL => ret!(),
            REPLY_REPLY => ret!(vals, Num,),
            KEY_NEW => ret!(vals, CapId,),
            KEY_ID => ret!(vals, Num,),
            DROP_CHECK_NEW => ret!(vals, CapId, CapId,),
            DROP_CHECK_RECIEVER_HANDLE_CAP_DROP_SYNC => ret!(vals, Num,),
            DROP_CHECK_RECIEVER_HANDLE_CAP_DROP_ASYNC => ret!(),
            MMIO_ALLOCATOR_ALLOC => ret!(vals, CapId,),
            PHYS_MEM_MAP => ret!(vals, Num,),
            PHYS_MEM_GET_SIZE => ret!(vals, Num,),
            _ => unreachable!(),
        };

        write!(out, "{})", ret_values).unwrap();

        out
    } else {
        if let Some(err) = SysErr::new(vals.a1) {
            format!(root_alloc_ref(), "Err(SysErr::{:?})", err)
        } else {
            String::from_str(root_alloc_ref(), "Err(<invalid syserr>)").unwrap()
        }
    }
}