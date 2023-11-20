use arpc::arpc_interface;

use crate::prelude::*;

#[arpc_interface(service_id = 1, name = "Fs")]
pub trait FsService {
}

pub enum FsEntry {
    File(File),
    Directory(Directory),
    Link(Link),
}

pub struct File {
    name: String,
}

pub struct Directory {
    name: String,
}

pub struct Link {
    name: String,
    target: String,
}