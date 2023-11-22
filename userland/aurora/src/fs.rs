use thiserror_no_std::Error;

use crate::{prelude::*, service::AppService};

#[derive(Debug, Error)]
pub enum FsError {
    InvalidPath,
}

//#[arpc::service(service_id = 2, name = "Fs", AppService = crate::service)]
pub trait FsService: AppService {
    fn access(&self, path: String) -> Result<FsEntry, FsError>;
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