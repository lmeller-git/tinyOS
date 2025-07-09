use alloc::sync::Arc;
use hashbrown::HashMap;

use crate::{
    bootinfo,
    drivers::graphics::{
        GLOBAL_FRAMEBUFFER, framebuffers::GlobalFrameBuffer, framebuffers::LimineFrameBuffer,
    },
    services::graphics::{PrimitiveDrawTarget, Simplegraphics},
};

use super::{FdEntry, FdTag, RawDeviceID, RawFdEntry};

pub struct GFXBuilder {
    id: RawDeviceID,
}

impl GFXBuilder {
    pub(super) fn new(id: RawDeviceID) -> Self {
        Self { id }
    }

    pub fn simple<T: FdTag>(self) -> FdEntry<T> {
        let mut new_map = HashMap::new();
        new_map.insert(
            self.id,
            Arc::new(Simplegraphics::new(&*GLOBAL_FRAMEBUFFER)) as Arc<dyn PrimitiveDrawTarget>,
        );
        FdEntry::new(RawFdEntry::GraphicsBackend(new_map), self.id)
    }
}
