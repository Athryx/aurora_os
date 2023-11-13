#![no_std]

#![feature(associated_type_defaults)]
#![feature(decl_macro)]
#![feature(async_fn_in_trait)]

use arpc::arpc_interface;

#[arpc_interface(service_id = 0, name = "Fs")]
pub trait FsServer {
    fn add(&self, a: usize, b: usize) -> usize;
}