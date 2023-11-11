use thiserror_no_std::Error;
use aser::{Value, AserError};
use serde::{Serialize, Deserialize};

use crate::prelude::*;
use crate::collections::HashMap;
use crate::sync::Once;

#[derive(Debug, Error)]
pub enum EnvError {
    #[error("Serialization error: {0}")]
    AserError(#[from] AserError),
    #[error("No argument with the given name exists")]
    InvalidNamedArg,
}

pub(super) static THIS_NAMESPACE: Once<Namespace> = Once::new();

pub fn this_namespace() -> &'static Namespace {
    THIS_NAMESPACE.get().expect("namespace not initialized")
}

pub fn args() -> &'static Args {
    &this_namespace().args
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Namespace {
    pub(super) args: Args,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Args {
    pub(super) positional_args: Vec<Value>,
    pub(super) named_args: HashMap<String, Value>,
}

impl Args {
    /// Clones the argument data, but does not clone capabilites
    pub fn clone_data(&self) -> Args {
        Args {
            positional_args: self.positional_args.clone(),
            named_args: self.named_args.clone(),
        }
    }

    pub fn named_arg<'a, T: Deserialize<'a>>(&'a self, name: &str) -> Result<T, EnvError> {
        let value = self.named_args.get(name).ok_or(EnvError::InvalidNamedArg)?;
        Ok(value.into_deserialize()?)
    }
}