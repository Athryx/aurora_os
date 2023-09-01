use aser::Value;
use serde::{Serialize, Deserialize};

use crate::prelude::*;
use crate::sync::Once;

pub(super) static THIS_NAMESPACE: Once<Namespace> = Once::new();

pub fn this_namespace() -> &'static Namespace {
    THIS_NAMESPACE.get().expect("namespace not initialized")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Namespace {
    pub(super) args: Args,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Args {
    pub(super) positional_args: Vec<Value>,
    pub(super) named_args: Value,
}