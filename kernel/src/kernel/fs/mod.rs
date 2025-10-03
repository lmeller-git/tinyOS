mod path;
pub mod procfs;
mod ramfs;
mod vfs;
use alloc::{boxed::Box, sync::Arc};
use core::{
    error,
    fmt::{Debug, Write},
};

use bitflags::bitflags;
pub use path::*;
use thiserror::Error;
mod fs_util;
pub use fs_util::*;

use crate::kernel::fd::File;

pub const PROCFS_PATH: &str = "/proc";
pub const RAMFS_PATH: &str = "/ram";

pub fn init() {
    procfs::init();
    vfs::init();
    mount(
        Path::new(RAMFS_PATH).into(),
        Arc::new(ramfs::RamFS::new()) as Arc<dyn FS>,
    )
    .expect("failed to mount ramfs");
    mount(
        Path::new(PROCFS_PATH).into(),
        Arc::new(procfs::ProcFS::new()) as Arc<dyn FS>,
    )
    .expect("failed to mount procfs");
}

pub fn fs() -> &'static impl FS {
    vfs::get()
}

pub type FSResult<T> = Result<T, FSError>;

pub trait FS: Debug + Send + Sync {
    fn open(&self, path: &Path, options: OpenOptions) -> FSResult<File>;
    fn unlink(&self, path: &Path, options: UnlinkOptions) -> FSResult<File>;
    fn flush(&self, path: &Path) -> FSResult<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    File,
    Dir,
    SymLink,
    Mount,
    Void,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OpenOptions: u32 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const APPEND = 1 << 2;
        const TRUNCATE = 1 << 3;
        const CREATE = 1 << 4;
        const CREATE_DIR = 1 << 5;
        const CREATE_ALL = 1 << 6;
        const CREATE_LINK = 1 << 7;
        const NO_FOLLOW_LINK = 1 << 8;
    }
}

impl OpenOptions {
    pub fn with_read(self) -> Self {
        self | Self::READ
    }

    pub fn with_write(self) -> Self {
        self | Self::WRITE
    }

    pub fn with_no_follow_symlink(self) -> Self {
        self | Self::NO_FOLLOW_LINK
    }

    pub fn with_truncate(self) -> Self {
        self | Self::TRUNCATE
    }

    pub fn with_append(self) -> Self {
        self | Self::APPEND
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::READ
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UnlinkOptions: u32 {
        const FORCE = 1 << 0;
        const RECURSIVE = 1 << 1;
        const NO_PRESERVE_ROOT = 1 << 2;
    }
}

impl UnlinkOptions {
    fn with_force(self) -> Self {
        self | Self::FORCE
    }

    fn with_rmdir(self) -> Self {
        self | Self::RECURSIVE
    }
}

impl Default for UnlinkOptions {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub struct FSError {
    repr: FSErrRepr,
}

impl FSError {
    pub fn simple(kind: FSErrorKind) -> Self {
        Self {
            repr: FSErrRepr::Simple(kind),
        }
    }

    pub fn with_message(kind: FSErrorKind, msg: &'static str) -> Self {
        Self {
            repr: FSErrRepr::SimpleMessage { msg, kind },
        }
    }

    pub fn custom(kind: FSErrorKind, err: Box<dyn error::Error + Send + Sync>) -> Self {
        Self {
            repr: FSErrRepr::Custom { kind, err },
        }
    }

    pub fn kind(&self) -> &FSErrorKind {
        match &self.repr {
            FSErrRepr::Simple(kind) => kind,
            FSErrRepr::SimpleMessage { msg: _, kind } => kind,
            FSErrRepr::Custom { kind, err: _ } => kind,
        }
    }
}

#[derive(Error, Debug)]
pub enum FSErrRepr {
    #[error(transparent)]
    Simple(FSErrorKind),
    #[error("Error with kind: {}, msg: {}", kind, msg)]
    SimpleMessage {
        msg: &'static str,
        kind: FSErrorKind,
    },
    #[error("Custom error with kind: {}, err: {}", kind, err)]
    Custom {
        kind: FSErrorKind,
        err: Box<dyn error::Error + Send + Sync>,
    },
}

#[derive(Error, Debug, PartialEq, Eq, Clone, Copy)]
pub enum FSErrorKind {
    #[error("Node not found")]
    NotFound,
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Node exists")]
    AlreadyExists,
    #[error("Op would block")]
    WouldBlock,
    #[error("Node not a directory")]
    NotADir,
    #[error("Node is a directory")]
    IsADir,
    #[error("Directory is not empty")]
    DirNotEmpty,
    #[error("Op timed out")]
    TimedOut,
    #[error("Storage full")]
    StorageFull,
    #[error("File too large")]
    FileTooLarge,
    #[error("would deadlock")]
    Deadlock,
    #[error("Invalid filename")]
    InvalidFilename,
    #[error("Invalid Path")]
    InvalidPath,
    #[error("Out of memory")]
    OOM,
    #[error("unexpected EOF")]
    UnexpectedEOF,
    #[error("Op is in progress")]
    InProgress,
    #[error("Unspecified")]
    Other,
    #[error("This Operation is not supported")]
    NotSupported,
}
