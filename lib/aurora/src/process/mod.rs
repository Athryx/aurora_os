use aser::Value;
use serde::Serialize;

use crate::prelude::*;
use crate::container::HashMap;

/// Where the elf data to launc hthe process is comming from
enum ProcessDataSource {
    Bytes(Vec<u8>),
}

#[derive(Default, Serialize)]
struct ArgsBuilder {
    positional_args: Vec<Value>,
    named_args: HashMap<String, Value>,
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

    pub fn arg<T: Serialize>(&mut self, arg: T) -> &mut Self {
        self
    }

    pub fn args<T: Serialize, I: IntoIterator<Item = T>>(&mut self, args: I) -> &mut Self {
        for arg in args {
            self.arg(arg);
        }

        self
    }
}