pub mod primitive;
pub mod thread_safe;

mod gkl;
pub use gkl::*;

pub static GKL: Gkl = Gkl::new();
