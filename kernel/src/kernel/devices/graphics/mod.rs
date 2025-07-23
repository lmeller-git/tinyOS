use core::{fmt::Debug, marker::PhantomData};

use alloc::{boxed::Box, sync::Arc};
use embedded_graphics::prelude::DrawTarget;
use hashbrown::HashMap;

use crate::{
    bootinfo,
    drivers::graphics::{
        GLOBAL_FRAMEBUFFER,
        colors::{ColorCode, RGBColor},
        framebuffers::{
            BoundingBox, FrameBuffer, GlobalFrameBuffer, HasFrameBuffer, LimineFrameBuffer,
            RawFrameBuffer,
        },
    },
    locks::reentrant::Mutex,
    serial_println,
    services::graphics::{
        BlitTarget, Glyph, GraphicsBackend, GraphicsError, PrimitiveDrawTarget, PrimitiveGlyph,
        Simplegraphics,
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

    //TODO refactor this to remove duplication (also in framebuffer.rs)
    pub fn blit_user<T: FdTag>(self, addr: crate::arch::mem::VirtAddr) -> FdEntry<T> {
        let intermediate = Simplegraphics::new(Box::leak(Box::new(unsafe {
            RawFrameBuffer::new_user(
                addr,
                GLOBAL_FRAMEBUFFER.width(),
                GLOBAL_FRAMEBUFFER.height(),
                GLOBAL_FRAMEBUFFER.bpp(),
            )
        })));

        let target = Simplegraphics::new(&*GLOBAL_FRAMEBUFFER);
        let entry = Arc::new(BlitManager::new(intermediate, target));
        FdEntry::new(RawFdEntry::GraphicsBackend(self.id, entry), self.id)
    }

    pub fn blit_kernel<T: FdTag>(self, addr: crate::arch::mem::VirtAddr) -> FdEntry<T> {
        let intermediate = Simplegraphics::new(Box::leak(Box::new(unsafe {
            RawFrameBuffer::new_kernel(
                addr,
                GLOBAL_FRAMEBUFFER.width(),
                GLOBAL_FRAMEBUFFER.height(),
                GLOBAL_FRAMEBUFFER.bpp(),
            )
        })));

        let target = Simplegraphics::new(&*GLOBAL_FRAMEBUFFER);
        let entry = Arc::new(BlitManager::new(intermediate, target));
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
    fn flush(&self, area: &BoundingBox) {}
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

pub struct BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
    intermediate: Mutex<B>,
    target: Mutex<T>,
    _phantom: PhantomData<F>,
}

impl<B, T, F> BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
    pub fn new(intermediate: B, target: T) -> Self {
        Self {
            intermediate: Mutex::new(intermediate),
            target: Mutex::new(target),
            _phantom: PhantomData,
        }
    }
}

impl<B, T, F> GFXManager for BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
    fn draw_primitive(&self, primitive: &PrimitiveGlyph) -> Result<(), GraphicsError> {
        primitive.render_in(&mut *self.intermediate.lock())
    }

    fn draw_batched_primitives(&self, primitives: &[&PrimitiveGlyph]) -> Result<(), GraphicsError> {
        serial_println!("blit drawing prim");
        let backend = &mut *self.intermediate.lock();
        for glyph in primitives {
            glyph.render_in(backend)?;
        }
        Ok(())
    }

    fn draw_glyph(&self, glyph: &dyn Glyph, color: &ColorCode) {
        glyph.render_colorized(color, &*self.intermediate.lock());
    }

    fn flush(&self, area: &BoundingBox) {
        self.target
            .lock()
            .copy_rect(area, self.intermediate.lock().get_framebuffer());
    }
}

impl<B, T, F> Debug for BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        todo!()
    }
}

unsafe impl<B, T, F> Send for BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
}

unsafe impl<B, T, F> Sync for BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
}
