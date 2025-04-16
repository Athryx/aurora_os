//! Scuffed in memory 'filesystem' for ctf
//! Just a flat list

use crate::prelude::*;
use crate::sync::IMutex;

pub struct File {
    name: Vec<u8>,
    owner: usize,
    data: Vec<u8>,
}

pub struct Fs {
    files: Vec<File>,
}

impl Fs {
    pub const fn new() -> Fs {
        Fs { files: Vec::new() }
    }

    pub fn get_file(&self, filename: &[u8]) -> Option<&File> {
        self.files.iter().find(|file| file.name == filename)
    }
}

pub static FS: IMutex<Fs> = IMutex::new(Fs::new());