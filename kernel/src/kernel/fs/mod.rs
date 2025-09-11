mod path;
mod procfs;
mod ramfs;
mod vfs;
use alloc::{boxed::Box, sync::Arc};
use core::{error, fmt::Debug};

pub use path::*;
use thiserror::Error;

use crate::kernel::fd::IOCapable;

pub enum FSNode {
    File(Arc<dyn File>),
    Dir(Arc<dyn Dir>),
    Link(Arc<dyn Link>),
    Fs(Arc<dyn FS>),
}

pub trait FS: Debug {
    fn open(&self, path: &Path) -> FSResult<FSNode>;
    fn close(&self, path: &Path) -> FSResult<()>;
    fn add_node(&self, path: &Path, node: FSNode) -> FSResult<()>;
    fn remove_node(&self, path: &Path) -> FSResult<FSNode>;
}

pub trait File: IOCapable + Debug {}

pub trait Dir: Debug {}

pub trait Link: IOCapable + Debug {}

pub type FSResult<T> = Result<T, FSError>;

#[derive(Error, Debug)]
#[error(transparent)]
pub struct FSError {
    repr: FSRepr,
}

impl FSError {
    pub fn simple(kind: FSErrorKind) -> Self {
        Self {
            repr: FSRepr::Simple(kind),
        }
    }

    pub fn with_message(kind: FSErrorKind, msg: &'static str) -> Self {
        Self {
            repr: FSRepr::SimpleMessage { msg, kind },
        }
    }

    pub fn custom(kind: FSErrorKind, err: Box<dyn error::Error + Send + Sync>) -> Self {
        Self {
            repr: FSRepr::Custom { kind, err },
        }
    }

    pub fn kind(&self) -> &FSErrorKind {
        match &self.repr {
            FSRepr::Simple(kind) => kind,
            FSRepr::SimpleMessage { msg: _, kind } => kind,
            FSRepr::Custom { kind, err: _ } => kind,
        }
    }
}

#[derive(Error, Debug)]
pub enum FSRepr {
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
    #[error("Out of memory")]
    OOM,
    #[error("unexpected EOF")]
    UnexpectedEOF,
    #[error("Op is in progress")]
    InProgress,
    #[error("Unspecified")]
    Other,
}
