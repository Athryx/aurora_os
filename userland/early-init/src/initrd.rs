use core::ptr;
use core::mem::size_of;

const INITRD_MAGIC_NUMBER: u64 = 0x39f298aa4b92e836;

#[repr(C)]
struct InitrdHeader {
    magic: u64,
    entry_list_len: u64,
}

#[repr(C)]
struct InitrdEntry {
    typ: u64,
    name_offset: u64,
    name_len: u64,
    data: u64,
    data_len: u64,
}

impl InitrdEntry {
    unsafe fn data(&self, initrd_base: usize) -> &'static [u8] {
        let data_ptr = (initrd_base + self.data as usize) as *const u8;

        unsafe {
            core::slice::from_raw_parts(data_ptr, self.data_len as usize)
        }
    }
}

const PART_LIST_TYPE: u64 = 2;
const FS_SERVER_TYPE: u64 = 3;

pub struct InitrdData {
    pub part_list: &'static [u8],
    pub fs_server: &'static [u8],
}

/// Gets relevant information from the initrd
/// 
/// # Safety
/// 
/// `initrd_address` must be the address of a valid initrd
// not very robust parsing, we just assume kernel gives us a valid initrd,
// there is nothing we can do other then panic if it is wrong
pub unsafe fn parse_initrd(initrd_address: usize) -> InitrdData {
    let header = unsafe {
        ptr::read(initrd_address as *const InitrdHeader)
    };

    assert_eq!(header.magic, INITRD_MAGIC_NUMBER, "invalid initrd magic number");

    let entry_list_ptr = (initrd_address + size_of::<InitrdHeader>()) as *const InitrdEntry;
    let entries = unsafe {
        core::slice::from_raw_parts(entry_list_ptr, header.entry_list_len as usize)
    };

    let mut part_list = None;
    let mut fs_server = None;

    for entry in entries {
        match entry.typ {
            PART_LIST_TYPE => {
                part_list = Some(entry.data(initrd_address));
            },
            FS_SERVER_TYPE => {
                fs_server = Some(entry.data(initrd_address));
            },
            _ => (),
        }
    }

    InitrdData {
        part_list: part_list.expect("no partition list found in initrd"),
        fs_server: fs_server.expect("no fs server found in initrd"),
    }
}