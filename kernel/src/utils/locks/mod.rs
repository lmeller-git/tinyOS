//TODO refactor this to reduce code duplication

mod gkl;

pub use gkl::*;
use thiserror::Error;

pub static GKL: Gkl = Gkl::new();

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum LockErr {
    #[error("Lock not free")]
    AlreadyLocked,
    #[cfg(feature = "gkl")]
    #[error("GKL is not free")]
    GKLHeld,
}
