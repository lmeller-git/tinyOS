//TODO refactor this to reduce code duplication

pub mod primitive;
pub mod reentrant;
pub mod thread_safe;

mod gkl;
pub use gkl::*;

pub static GKL: Gkl = Gkl::new();
