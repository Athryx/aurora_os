pub trait ByteBuf: Default {
    fn push(&mut self, byte: u8);
    fn extend_from_slice(&mut self, slice: &[u8]);
    fn as_slice(&mut self) -> &mut [u8];
    fn len(&self) -> usize;
}

#[cfg(feature = "alloc")]
impl ByteBuf for alloc::vec::Vec<u8> {
    fn push(&mut self, byte: u8) {
        self.push(byte);
    }

    fn extend_from_slice(&mut self, slice: &[u8]) {
        self.extend_from_slice(slice);
    }

    fn as_slice(&mut self) -> &mut [u8] {
        &mut self[..]
    }

    fn len(&self) -> usize {
        self.len()
    }
}