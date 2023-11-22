#![no_std]

#![feature(associated_type_defaults)]
#![feature(decl_macro)]
#![feature(async_fn_in_trait)]

#[arpc::service(service_id = 11, name = "Fs")]
pub trait FsServer {
    fn add(&self, a: usize, b: usize) -> usize;
}