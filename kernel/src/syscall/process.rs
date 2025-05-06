use crate::prelude::*;

use super::copy_vec_from_userspace;
use crate::fs::FS;

pub fn process_spawn(options: u32, path: usize, path_len: usize) -> KResult<usize> {
    let file_path = copy_vec_from_userspace(path as *const u8, path_len)?;
    let fs = FS.lock();
    let file = fs.get_file(&file_path)
        .ok_or(SysErr::NotFound)?;
}

pub fn process_send_message(options: u32, data: usize, data_len: usize) -> KResult<()> {
    todo!()
}

pub fn process_recv_message(options: u32, data: usize, data_len: usize) -> KResult<usize> {
    todo!()
}

pub fn process_set_uid(options: u32, new_uid: usize) -> KResult<()> {
    todo!()
}

pub fn process_map_mem(options: u32, address: usize, size: usize) -> KResult<()> {
    todo!()
}

pub fn process_unmap_mem(options: u32, address: usize) -> KResult<()> {
    todo!()
}