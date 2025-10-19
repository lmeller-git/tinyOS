use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use core::{
    fmt::{self, Debug},
    ops::Deref,
    sync::atomic::{AtomicUsize, Ordering},
};

use bitflags::bitflags;

use crate::{
    arch::x86::current_time,
    kernel::{
        fs::{FSError, FSErrorKind, NodeType, OpenOptions, PathBuf},
        io::{IOResult, Read, Write},
    },
};

pub type FileDescriptor = u32;
pub type FDMap = BTreeMap<FileDescriptor, Arc<File>>;

pub const STDIN_FILENO: FileDescriptor = 0;
pub const STDOUT_FILENO: FileDescriptor = 1;
pub const STDERR_FILENO: FileDescriptor = 2;

pub trait IOCapable: Read + Write {}

pub trait FileRepr: Debug + IOCapable + Send + Sync {
    fn fstat(&self) -> FStat {
        FStat::new()
    }

    fn node_type(&self) -> NodeType;
}

#[macro_export]
macro_rules!  impl_file_for_wr {
    (@impl [$($impl_generics:tt)*] $name:ty: $node:expr) => {
        impl<$($impl_generics)*> $crate::kernel::fd::FileRepr for $name {
            fn node_type(&self) -> NodeType {
                $node
            }
        }

        impl<$($impl_generics)*> $crate::kernel::fd::IOCapable for $name {}
    };

    ($name:ty: $node:expr) => {
        impl_file_for_wr!(@impl [] $name: $node);
    };

    ($name:ty where [$($generics:tt)*]: $node:expr) => {
        impl_file_for_wr!(@impl [$($generics)*] $name: $node);
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FStat {
    pub t_create: u64,
    pub t_mod: u64,
    pub size: usize,
}

impl FStat {
    pub fn new() -> Self {
        let now = current_time().as_secs();
        Self {
            t_create: now,
            t_mod: now,
            size: 0,
        }
    }
}

bitflags! {
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct FPerms: u8 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
    }
}

impl From<OpenOptions> for FPerms {
    fn from(value: OpenOptions) -> Self {
        if value.contains(OpenOptions::WRITE) {
            Self::WRITE
        } else if value.contains(OpenOptions::READ) {
            Self::READ
        } else {
            Self::empty()
        }
    }
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

    pub fn into_shared(self) -> Self {
        match self {
            Self::Owned(t) => Self::Shared(t.into()),
            Self::Shared(_) => self,
        }
    }

    pub fn try_clone(&self) -> Option<Self> {
        match self {
            Self::Owned(_) => None,
            Self::Shared(t) => Some(Self::Shared(t.clone())),
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

impl<T> Clone for MaybeOwned<T>
where
    T: ?Sized + Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Owned(t) => Self::Owned(t.clone()),
            Self::Shared(t) => Self::Shared(t.clone()),
        }
    }
}

unsafe impl<T> Sync for MaybeOwned<T> where T: Sync + ?Sized {}
unsafe impl<T> Send for MaybeOwned<T> where T: Send + ?Sized {}

#[derive(Debug)]
pub struct File {
    repr: MaybeOwned<dyn FileRepr>,
    cursor: FCursor,
    perms: FPerms,
}

impl File {
    pub fn new(repr: impl Into<MaybeOwned<dyn FileRepr>>) -> Self {
        Self {
            repr: repr.into(),
            cursor: FCursor::default(),
            perms: FPerms::empty(),
        }
    }

    pub fn new_shareable(repr: impl Into<MaybeOwned<dyn FileRepr>>) -> Self {
        let repr: MaybeOwned<_> = repr.into();
        Self::new(repr.into_shared())
    }

    pub fn with_perms<T>(mut self, perms: T) -> Self
    where
        FPerms: From<T>,
    {
        self.perms |= perms.into();
        self
    }

    pub fn read_continuous(&self, buf: &mut [u8]) -> super::io::IOResult<usize> {
        let n = self.read(buf, self.cursor.get())?;
        self.cursor.advance(n);
        Ok(n)
    }

    pub fn write_continuous(&self, buf: &[u8]) -> super::io::IOResult<usize> {
        let n = self.write(buf, self.cursor.get())?;
        self.cursor.advance(n);
        Ok(n)
    }

    pub fn set_cursor(&self, offset: usize) {
        self.cursor.inner.store(offset, Ordering::Release);
    }

    pub fn may_write(&self) -> bool {
        self.perms.contains(FPerms::WRITE)
    }

    pub fn may_read(&self) -> bool {
        self.perms.contains(FPerms::READ) || self.may_write()
    }

    pub fn read_all_as_str(&self) -> IOResult<String> {
        let mut buf = String::new();
        self.read_to_string(&mut buf, 0)?;
        Ok(buf)
    }

    pub fn try_clone_without_offset(&self) -> Option<Self> {
        Some(Self {
            repr: self.repr.try_clone()?,
            cursor: FCursor::default(),
            perms: self.perms.clone(),
        })
    }
}

impl FileRepr for File {
    fn fstat(&self) -> FStat {
        self.repr.fstat()
    }

    fn node_type(&self) -> NodeType {
        self.repr.node_type()
    }
}

impl IOCapable for File {}

impl Read for File {
    fn read(&self, buf: &mut [u8], offset: usize) -> super::io::IOResult<usize> {
        if !self.may_read() {
            return Err(FSError::simple(FSErrorKind::PermissionDenied));
        }
        self.repr.read(buf, offset)
    }

    fn read_to_end(&self, buf: &mut Vec<u8>, offset: usize) -> super::io::IOResult<usize> {
        self.repr.read_to_end(buf, offset)
    }
}

impl Write for File {
    fn write(&self, buf: &[u8], offset: usize) -> super::io::IOResult<usize> {
        if !self.may_write() {
            return Err(FSError::simple(FSErrorKind::PermissionDenied));
        }
        self.repr.write(buf, offset)
    }

    fn write_all(&self, buf: &[u8], offset: usize) -> super::io::IOResult<()> {
        self.repr.write_all(buf, offset)
    }
}

impl fmt::Write for File {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        self.write_all(bytes, 0)
            .map_err(|_| fmt::Error::default())?;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct FCursor {
    inner: AtomicUsize,
}

impl FCursor {
    pub fn advance(&self, n: usize) {
        self.inner.fetch_add(n, Ordering::Release);
    }

    pub fn get(&self) -> usize {
        self.inner.load(Ordering::Acquire)
    }
}

impl Clone for FCursor {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.load(Ordering::Acquire).into(),
        }
    }
}
