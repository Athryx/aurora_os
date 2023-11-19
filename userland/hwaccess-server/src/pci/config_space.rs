pub const CONFIG_SPACE_SIZE: usize = 4096;

pub const VENDOR_ID_INVALID: u16 = 0xffff;

// FIXME: get this to be packed without causing compile error in map_field macro
#[repr(C)]
pub struct PciConfigSpace {
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub cache_line_size: u8,
    pub latency_timer: u8,
    pub header_type: u8,
    pub bist: u8,
}