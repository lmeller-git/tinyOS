use framebuffers::GlobalFrameBuffer;
use lazy_static::lazy_static;

pub mod colors;
pub mod framebuffers;
pub mod text;

lazy_static! {
    pub static ref GLOBAL_FRAMEBUFFER: GlobalFrameBuffer = GlobalFrameBuffer::new_static();
}
