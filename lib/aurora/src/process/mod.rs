use crate::allocator::addr_space::{RemoteAddrSpaceManager, AddrSpaceError};
use crate::context::Context;
use crate::env::{Args, Namespace};

use aser::Value;
use elf::{ElfBytes, ParseError};
use elf::endian::NativeEndian;
use serde::Serialize;
use sys::{Allocator, CapFlags, SysErr};
use thiserror_no_std::Error;

use crate::{prelude::*, this_context};
use crate::collections::HashMap;

#[derive(Error)]
pub enum ProcessError {
    #[error("System error: {0}")]
    SysErr(#[from] SysErr),
    #[error("Error parsing elf data: {0}")]
    ElfParseError(#[from] ParseError),
    #[error("Error mapping memory in new process: {0}")]
    AddrSpaceError(#[from] AddrSpaceError),
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
        let current_context = this_context();

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

    pub fn spawn(&mut self) -> Result<Process, ProcessError> {
        let namespace = Namespace {
            args: Args::from(&self.args),
        };

        let exe_data = self.process_data.bytes();

        spawn_process(exe_data, namespace)
    }
}

fn spawn_process(exe_data: &[u8], namespace: Namespace) -> Result<Process, ProcessError> {
    let aslr_seed = gen_aslr_seed();

    let process = Process::new(CapFlags::all(), allocator, spawner)?;
    let context = Context {
        process,
        allocator,
        spawner,
    };

    let manager = RemoteAddrSpaceManager::new_remote(aslr_seed, context)?;

    let elf_data = ElfBytes::<NativeEndian>::minimal_parse(exe_data)?;

    todo!()
}

fn gen_aslr_seed() -> [u8; 32] {
    // TODO: implement once randomness is a thing
    [12, 64, 89, 134, 11, 235, 123, 98, 12, 31, 2, 90, 38, 24, 3, 49, 32, 58, 238, 210, 1, 0, 24, 23, 9, 48, 28, 65, 1, 43, 54, 55]
}