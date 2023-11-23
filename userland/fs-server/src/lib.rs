#![no_std]

#![feature(associated_type_defaults)]
#![feature(decl_macro)]

#[arpc::service(service_id = 11, name = "Fs")]
pub trait FsServer {
    fn add(&self, a: usize, b: usize) -> usize;
}