use strum::FromRepr;

/// `IpcDataLayout` describes the layout and any processing of data the kernel
/// should do on messages when they are transferred.
#[repr(C)]
pub struct IpcDataLayout {
    /// Size of data in bytes.
    /// 
    /// Must be 8 byte aligned
    size: usize,
    fixups: *const IpcFixup,
    fixup_count: usize,
}

#[repr(u8)]
#[derive(FromRepr)]
pub enum IpcFixupTypes {
    /// Transfer a capability to the other process.
    Capability = 0,
    /// Transfer data element points to to another process.
    /// 
    /// If pointer in data is also allowed to be null.
    Ptr = 1,
    /// Fixes up 2 words, one which is a pointer and one is a length.
    /// Transfers length elements to the other process.
    PtrLen = 2,
    /// Same a `PtrLen`, but 3rd element is a capacity.
    /// 
    /// Used to transfer owned Vecs, capacity field is set the same as length field.
    PtrLenCapacity = 3,
}

#[repr(C)]
pub struct IpcFixup {
    /// Byte offset of element to fix up
    /// 
    /// Must be 8 byte aligned
    offset: usize,
    fixup_type: u8,
    /// Child layout may be null
    child_layout: *const IpcDataLayout,
}