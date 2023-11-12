use serde::Serialize;
use aser::{Value, to_bytes_count_cap};
pub use aurora_core::process::{Child, ProcessError, exit};
use aurora_core::process::spawn_process;
use aurora_core::prelude::*;

use crate::env::{Namespace, Args};

/// Where the elf data to launch the process is comming from
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

/// Used to execute other processess
/// 
/// Functions similarly to the standard library's Command
pub struct Command {
    process_data: ProcessDataSource,
    args: Args,
}

impl Command {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Command {
            process_data: ProcessDataSource::Bytes(bytes),
            args: Args::default(),
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
            // it is fine for only data to be cloned,
            // spawn_process will transfer necessary capabilities
            args: self.args.clone_data(),
        };

        let exe_data = self.process_data.bytes();
        let mut namespace_data: Vec<u8> = to_bytes_count_cap(&namespace)?;

        spawn_process(exe_data, &mut namespace_data)
    }
}