use core::fmt::Debug;

use alloc::{boxed::Box, sync::Arc};
use embedded_graphics::prelude::DrawTarget;
use hashbrown::HashMap;

use crate::{
    bootinfo,
    drivers::graphics::{
        GLOBAL_FRAMEBUFFER,
        colors::{ColorCode, RGBColor},
        framebuffers::{GlobalFrameBuffer, LimineFrameBuffer},
    },
    locks::reentrant::Mutex,
    services::graphics::{
        Glyph, GraphicsBackend, GraphicsError, PrimitiveDrawTarget, PrimitiveGlyph, Simplegraphics,
    },
};

use super::{FdEntry, FdTag, Null, RawDeviceID, RawFdEntry};

pub struct GFXBuilder {
    id: RawDeviceID,
}

impl GFXBuilder {
    pub(super) fn new(id: RawDeviceID) -> Self {
        Self { id }
    }

    pub fn simple<T: FdTag>(self) -> FdEntry<T> {
        let entry = Arc::new(SimpleGFXManager::new(Simplegraphics::new(
            &*GLOBAL_FRAMEBUFFER,
        )));
        FdEntry::new(RawFdEntry::GraphicsBackend(self.id, entry), self.id)
    }
}

impl PrimitiveDrawTarget for Null {
    fn draw_primitive(
        &mut self,
        item: &crate::services::graphics::PrimitiveGlyph<'_>,
    ) -> Result<(), crate::services::graphics::GraphicsError> {
        Ok(())
    }
    fn clear(
        &mut self,
        color: crate::drivers::graphics::colors::RGBColor,
    ) -> Result<(), crate::services::graphics::GraphicsError> {
        Ok(())
    }
}

impl GFXManager for Null {
    fn draw_primitive(&self, primitive: &PrimitiveGlyph) -> Result<(), GraphicsError> {
        Ok(())
    }

    fn draw_batched_primitives(&self, primitives: &[&PrimitiveGlyph]) -> Result<(), GraphicsError> {
        Ok(())
    }
    fn draw_glyph(&self, glyph: &dyn Glyph, color: &ColorCode) {}
}

pub trait GFXManager: Debug {
    fn draw_primitive(&self, primitive: &PrimitiveGlyph) -> Result<(), GraphicsError>;
    fn draw_batched_primitives(&self, primitives: &[&PrimitiveGlyph]) -> Result<(), GraphicsError>;
    fn draw_glyph(&self, glyph: &dyn Glyph, color: &ColorCode);
}

pub struct SimpleGFXManager<B>
where
    B: PrimitiveDrawTarget + DrawTarget<Color = RGBColor, Error = GraphicsError>,
{
    inner: Mutex<B>,
}

impl<B> SimpleGFXManager<B>
where
    B: PrimitiveDrawTarget + DrawTarget<Color = RGBColor, Error = GraphicsError> + GraphicsBackend,
{
    fn new(backend: B) -> Self {
        Self {
            inner: Mutex::new(backend),
        }
    }
}

impl<B> GFXManager for SimpleGFXManager<B>
where
    B: PrimitiveDrawTarget + DrawTarget<Color = RGBColor, Error = GraphicsError> + GraphicsBackend,
{
    fn draw_primitive(&self, primitive: &PrimitiveGlyph) -> Result<(), GraphicsError> {
        primitive.render_in(&mut *self.inner.lock())
    }

    fn draw_batched_primitives(&self, primitives: &[&PrimitiveGlyph]) -> Result<(), GraphicsError> {
        let backend = &mut *self.inner.lock();
        for glyph in primitives {
            glyph.render_in(backend)?;
        }
        Ok(())
    }

    fn draw_glyph(&self, glyph: &dyn Glyph, color: &ColorCode) {
        glyph.render_colorized(color, &*self.inner.lock());
    }
}

impl<B> Debug for SimpleGFXManager<B>
where
    B: PrimitiveDrawTarget + DrawTarget<Color = RGBColor, Error = GraphicsError> + GraphicsBackend,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SimpleGFXManager")
    }
}
