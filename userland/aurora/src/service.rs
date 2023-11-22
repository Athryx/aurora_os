use alloc::string::String;

use sys::Key;
use serde::{Serialize, Deserialize};

use crate::prelude::*;

#[arpc::service(service_id = 1, name = "Service")]
pub trait AppService {
    /// Gets the permissions of this service instance
    fn get_permissions(&self) -> Vec<NamedPermission>;

    /// Creates a new sesssion with the given permissions
    /// 
    /// Permissions are anded to create the new session
    fn new_session_permissions(&self, permissions: Vec<Key>) -> Service;
}

#[derive(Serialize, Deserialize)]
pub struct NamedPermission {
    name: String,
    key: Key,
}