use alloc::sync::Arc;
use core::fmt::{self, Debug};

use crate::kernel::io::{Read, Write};

pub type FileDescriptor = u32;

pub const STDIN_FILENO: FileDescriptor = 0;
pub const STDOUT_FILENO: FileDescriptor = 1;
pub const STDERR_FILENO: FileDescriptor = 2;

pub trait IOCapable: Read + Write {}

pub trait FileRepr: Debug + IOCapable {
    fn fstat(&self) -> FStat;
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FStat {
    t_create: u64,
    t_mod: u64,
    size: usize,
}

#[derive(Debug)]
pub struct File {
    repr: Arc<dyn FileRepr>,
    cursor: FCursor,
}

impl File {
    pub fn new(repr: Arc<dyn FileRepr>) -> Self {
        Self {
            repr,
            cursor: FCursor::default(),
        }
    }

    pub fn read_continuous(&mut self, buf: &mut [u8]) -> super::io::IOResult<usize> {
        let n = self.read(buf, self.cursor.inner)?;
        self.cursor.advance(n);
        Ok(n)
    }

    pub fn write_continuous(&mut self, buf: &[u8]) -> super::io::IOResult<usize> {
        let n = self.write(buf, self.cursor.inner)?;
        self.cursor.advance(n);
        Ok(n)
    }

    pub fn set_cursor(&mut self, offset: usize) {
        self.cursor.inner = offset;
    }
}

impl FileRepr for File {
    fn fstat(&self) -> FStat {
        self.repr.fstat()
    }
}

impl IOCapable for File {}

impl Read for File {
    fn read(&self, buf: &mut [u8], offset: usize) -> super::io::IOResult<usize> {
        self.repr.read(buf, offset)
    }
}

impl Write for File {
    fn write(&self, buf: &[u8], offset: usize) -> super::io::IOResult<usize> {
        self.repr.write(buf, offset)
    }
}

impl fmt::Write for File {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        todo!()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FCursor {
    inner: usize,
}

impl FCursor {
    pub fn advance(&mut self, n: usize) {
        self.inner += n
    }
}
