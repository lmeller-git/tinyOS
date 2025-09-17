mod path;
mod procfs;
mod ramfs;
mod vfs;
use alloc::{boxed::Box, sync::Arc};
use core::{error, fmt::Debug};

use bitflags::bitflags;
pub use path::*;
use thiserror::Error;

use crate::kernel::fd::{File, IOCapable};

pub fn init() {
    procfs::init();
    ramfs::init();
    vfs::init();
}

pub type FSResult<T> = Result<T, FSError>;

pub trait FS: Debug + Send + Sync {
    fn open(&self, path: &Path, options: OpenOptions) -> FSResult<File>;
    fn unlink(&self, path: &Path, options: UnlinkOptions) -> FSResult<File>;
    fn flush(&self, path: &Path) -> FSResult<()>;
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
        const NO_FOLLOW_LINK = 1 << 7;
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
