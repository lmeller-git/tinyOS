use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{
    fmt::{self, Debug},
    ops::{Deref, DerefMut},
};

use crate::kernel::io::{Read, Write};

pub type FileDescriptor = u32;
pub type FDMap = Vec<File>;

pub const STDIN_FILENO: FileDescriptor = 0;
pub const STDOUT_FILENO: FileDescriptor = 1;
pub const STDERR_FILENO: FileDescriptor = 2;

pub trait IOCapable: Read + Write {}

pub trait FileRepr: Debug + IOCapable + Send + Sync {
    fn fstat(&self) -> FStat;
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FStat {
    t_create: u64,
    t_mod: u64,
    size: usize,
}

pub enum MaybeOwned<T: ?Sized> {
    Owned(Box<T>),
    Shared(Arc<T>),
}

impl<T: ?Sized> MaybeOwned<T> {
    pub fn new<V>(value: V) -> Self
    where
        MaybeOwned<T>: From<V>,
    {
        value.into()
    }

    pub fn into_shared(mut self) -> Self {
        match self {
            Self::Owned(t) => Self::Shared(t.into()),
            Self::Shared(_) => self,
        }
    }
}

impl<T: ?Sized> From<Arc<T>> for MaybeOwned<T> {
    fn from(value: Arc<T>) -> Self {
        Self::Shared(value)
    }
}

impl<T: ?Sized> From<Box<T>> for MaybeOwned<T> {
    fn from(value: Box<T>) -> Self {
        Self::Owned(value)
    }
}

impl<T> From<T> for MaybeOwned<T> {
    fn from(value: T) -> Self {
        Self::Owned(value.into())
    }
}

impl<T: ?Sized> Deref for MaybeOwned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(t) => t.as_ref(),
            Self::Shared(t) => t.as_ref(),
        }
    }
}

impl<T: ?Sized> AsRef<T> for MaybeOwned<T> {
    fn as_ref(&self) -> &T {
        self.deref()
    }
}

impl<T> Debug for MaybeOwned<T>
where
    T: Debug + ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

unsafe impl<T> Sync for MaybeOwned<T> where T: Sync + ?Sized {}
unsafe impl<T> Send for MaybeOwned<T> where T: Send + ?Sized {}

#[derive(Debug)]
pub struct File {
    repr: MaybeOwned<dyn FileRepr>,
    cursor: FCursor,
}

impl File {
    pub fn new<V>(repr: V) -> Self
    where
        MaybeOwned<dyn FileRepr>: From<V>,
    {
        Self {
            repr: repr.into(),
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
