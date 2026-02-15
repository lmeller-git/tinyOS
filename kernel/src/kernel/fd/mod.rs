use alloc::{
    boxed::Box,
    collections::btree_map::{BTreeMap, Values},
    string::String,
    sync::Arc,
    vec::Vec,
};
use core::{
    fmt::{self, Debug},
    ops::Deref,
    ptr::null_mut,
    sync::atomic::{AtomicUsize, Ordering},
};

use bitflags::bitflags;
pub use tinyos_abi::{
    consts::{STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO},
    types::FileDescriptor,
};

use crate::{
    arch::{self, x86::current_time},
    eprintln,
    kernel::{
        fs::{FSError, FSErrorKind, NodeType, OpenOptions, Path, PathBuf},
        io::{IOResult, Read, Write},
        threading::wait::{QueuTypeCondition, QueueType},
    },
};

pub type FDMap = BTreeMap<FileDescriptor, FileHandle>;

#[derive(Debug)]
pub struct FileHandle {
    f: Arc<File>,
}

impl<T> AsRef<T> for FileHandle
where
    T: ?Sized,
    <FileHandle as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}

impl Deref for FileHandle {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        self.f.deref()
    }
}

impl Drop for FileHandle {
    fn drop(&mut self) {
        self.f.repr.on_drop(FileMetadata {
            path: self.f.path.clone(),
            cursor: self.f.cursor.clone(),
            perms: self.f.perms.clone(),
        });
    }
}

impl Clone for FileHandle {
    fn clone(&self) -> Self {
        self.f.repr.on_clone(FileMetadata {
            path: self.f.path.clone(),
            cursor: self.f.cursor.clone(),
            perms: self.f.perms.clone(),
        });
        Self { f: self.f.clone() }
    }
}

impl From<Arc<File>> for FileHandle {
    fn from(value: Arc<File>) -> Self {
        Self { f: value }
    }
}

impl From<File> for FileHandle {
    fn from(value: File) -> Self {
        Self::from(Arc::new(value))
    }
}

impl From<Box<File>> for FileHandle {
    fn from(value: Box<File>) -> Self {
        let arc = Arc::new(Box::into_inner(value));
        arc.into()
    }
}

pub trait IOCapable: Read + Write {}

pub trait FileRepr: Debug + IOCapable + Send + Sync {
    fn node_type(&self) -> NodeType;
    fn fstat(&self) -> FStat {
        FStat::new()
    }

    fn clear(&self) -> IOResult<()> {
        Err(FSError::simple(FSErrorKind::NotSupported))
    }

    fn as_raw_parts(&self) -> (*mut u8, usize) {
        eprintln!(
            "called default FileRepr::as_raw_parts implementation. This is not what you want."
        );
        (null_mut(), 0)
    }

    fn get_waiter(&self) -> Option<QueuTypeCondition> {
        None
    }

    fn on_open(&self, _meta: FileMetadata) {}
    /// runs when ANY handle around this file clones
    fn on_clone(&self, _meta: FileMetadata) {}
    /// runs when ANY handle around this file drops
    fn on_drop(&self, _meta: FileMetadata) {}
    /// runs when the actual File gets dropped, ie the refcount reaches 0
    fn on_close(&self, _meta: FileMetadata) {}
}

pub trait FileReprFactory: Debug + Send + Sync {
    fn get_file_impl(&self) -> Result<Box<dyn FileRepr>, FSError>;
    fn get_file(&self) -> Result<File, FSError> {
        self.get_file_impl().map(File::new)
    }
}

pub struct FileMetadata {
    pub path: Option<PathBuf>,
    pub cursor: FCursor,
    pub perms: FPerms,
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

// this is very hacky, we should do the append/truncate stuff ONLY on file creation, not on with_perms. Should not be a FilePerm. TODO
bitflags! {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct FPerms: u8 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const APPEND = 1 << 2;
        const TRUNCATE = 1 << 3;
    }
}

impl Default for FPerms {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<OpenOptions> for FPerms {
    fn from(value: OpenOptions) -> Self {
        let mut zelf = FPerms::empty();
        if value.contains(OpenOptions::READ) {
            zelf |= FPerms::READ;
        }
        if value.contains(OpenOptions::WRITE) {
            zelf |= FPerms::WRITE;
        }
        if value.contains(OpenOptions::APPEND) {
            zelf |= FPerms::APPEND;
        }
        if value.contains(OpenOptions::TRUNCATE) {
            zelf |= FPerms::TRUNCATE;
        }
        zelf
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

    /// this is not atomic
    pub fn count(&self) -> usize {
        match self {
            Self::Owned(_) => 1,
            Self::Shared(s) => Arc::strong_count(s),
        }
    }

    pub fn make_shared(&mut self) {
        match self {
            Self::Owned(_) => {
                let o = unsafe { core::mem::replace(self, core::mem::zeroed()) };
                *self = o.into_shared();
            }
            Self::Shared(_) => {}
        }
    }

    pub fn try_clone(&self) -> Option<Self> {
        match self {
            Self::Owned(_) => None,
            Self::Shared(t) => Some(Self::Shared(t.clone())),
        }
    }

    pub fn try_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Owned(owned) => Some(owned),
            Self::Shared(_) => None,
        }
    }
}

impl<T> From<T> for MaybeOwned<T> {
    fn from(value: T) -> Self {
        Self::Owned(value.into())
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

pub struct FileBuilder {
    inner: File,
}

impl FileBuilder {
    pub fn new(repr: impl Into<MaybeOwned<dyn FileRepr>>) -> Self {
        Self {
            inner: File::new(repr),
        }
    }

    pub fn with_perms<T>(mut self, perms: T) -> Self
    where
        FPerms: From<T>,
    {
        self.inner = self.inner.with_perms(perms);
        self
    }

    pub fn shareable(mut self) -> Self {
        self.inner.repr.make_shared();
        self
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.inner = self.inner.with_path(path);
        self
    }

    pub fn finish(mut self) -> File {
        self.inner.repr.on_open(FileMetadata {
            path: self.inner.path.clone(),
            cursor: self.inner.cursor.clone(),
            perms: self.inner.perms.clone(),
        });
        self.inner
    }
}

#[derive(Debug)]
pub struct File {
    repr: MaybeOwned<dyn FileRepr>,
    cursor: FCursor,
    perms: FPerms,
    path: Option<PathBuf>,
}

impl File {
    fn new(repr: impl Into<MaybeOwned<dyn FileRepr>>) -> Self {
        Self {
            repr: repr.into(),
            cursor: FCursor::default(),
            perms: FPerms::empty(),
            path: None,
        }
    }

    fn new_shareable(repr: impl Into<MaybeOwned<dyn FileRepr>>) -> Self {
        let repr: MaybeOwned<_> = repr.into();
        Self::new(repr.into_shared())
    }

    fn with_perms<T>(mut self, perms: T) -> Self
    where
        FPerms: From<T>,
    {
        self.perms |= perms.into();
        if self.perms.contains(FPerms::APPEND) {
            self.set_cursor(self.repr.fstat().size);
        }
        if self.perms.contains(FPerms::WRITE | FPerms::TRUNCATE) {
            _ = self.repr.clear();
            self.set_cursor(0);
        }
        self
    }

    fn with_path(mut self, path: PathBuf) -> Self {
        self.path.replace(path);
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

    // TODO this should only be allowed if we have read permission (for channels, ...)
    // However a lot of code currently assumes otherwise. This needs to be reworked
    pub fn may_read(&self) -> bool {
        self.perms.contains(FPerms::READ) || self.may_write()
    }

    pub fn get_path(&self) -> Option<&Path> {
        self.path.as_ref().map(|p| &**p)
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
            path: self.path.clone(),
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

    fn as_raw_parts(&self) -> (*mut u8, usize) {
        let (ptr, len) = self.repr.as_raw_parts();
        let offset = self.cursor.get().min(len);
        (unsafe { ptr.offset(offset as isize) }, len - offset)
    }

    fn get_waiter(&self) -> Option<QueuTypeCondition> {
        if let Some(path) = &self.path {
            Some(QueuTypeCondition::new(QueueType::file(path)))
        } else {
            self.repr.get_waiter()
        }
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

impl Drop for File {
    fn drop(&mut self) {
        self.repr.on_close(FileMetadata {
            path: self.path.take(),
            cursor: core::mem::take(&mut self.cursor),
            perms: core::mem::take(&mut self.perms),
        });
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
