#![no_std]

extern crate std;

use std::prelude::*;

async fn sum(a: u32) -> u32 {
    a + 97
}

async fn test() {
    dprintln!("sum {}", sum(32).await);
}

fn main() {
    dprintln!("hello fs");

    aurora::async_runtime::block_in_place(async {
        test().await;
    });
}
