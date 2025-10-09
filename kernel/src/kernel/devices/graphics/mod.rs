use alloc::{boxed::Box, format, sync::Arc};
use core::{fmt::Debug, marker::PhantomData};

use conquer_once::spin::OnceCell;
use embedded_graphics::prelude::DrawTarget;

use super::*;
use crate::{
    arch::mem::VirtAddr,
    create_device_file,
    drivers::graphics::{
        GLOBAL_FRAMEBUFFER,
        colors::{ColorCode, RGBColor},
        framebuffers::{
            BoundingBox,
            FrameBuffer,
            GlobalFrameBuffer,
            HasFrameBuffer,
            RawFrameBuffer,
            get_config,
        },
    },
    kernel::{
        fd::{FileRepr, IOCapable},
        fs::{OpenOptions, Path, open},
        io::{IOError, Read, Write},
    },
    register_device_file,
    serial_println,
    services::graphics::{
        BlitTarget,
        Glyph,
        GraphicsBackend,
        GraphicsError,
        PrimitiveDrawTarget,
        PrimitiveGlyph,
        Simplegraphics,
    },
    sync::locks::Mutex,
};

pub static KERNEL_GFX_MANAGER: OnceCell<
    Arc<
        BlitManager<
            Simplegraphics<RawFrameBuffer>,
            Simplegraphics<GlobalFrameBuffer>,
            RawFrameBuffer,
        >,
    >,
> = OnceCell::uninit();

pub(super) fn init() {
    let addr = VirtAddr::new(0xffff_ffff_f000_0000);
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
    KERNEL_GFX_MANAGER.try_init_once(|| entry.clone());
    let _file = create_device_file!(
        entry,
        "/gfx/manager",
        OpenOptions::CREATE_ALL | OpenOptions::READ
    )
    .unwrap();

    let basic_config = get_config();
    let fb = &GLOBAL_FRAMEBUFFER;

    let mut gfx_config_file = open(
        Path::new("/ram/.devconf/gfx/config.conf"),
        OpenOptions::CREATE_ALL | OpenOptions::WRITE,
    )
    .unwrap();

    let fmt_str = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        basic_config.red_mask_shift,
        basic_config.red_mask_size,
        basic_config.green_mask_shift,
        basic_config.green_mask_size,
        basic_config.blue_mask_shift,
        basic_config.blue_mask_size,
        fb.bpp(),
        fb.width(),
        fb.height(),
        fb.pitch()
    );
    let bytes = fmt_str.as_bytes();

    gfx_config_file.write_all(bytes, 0).unwrap();
}

pub fn simple_manager() -> SimpleGFXManager<Simplegraphics<'static, GlobalFrameBuffer>> {
    SimpleGFXManager::new(Simplegraphics::new(&*GLOBAL_FRAMEBUFFER))
}

//TODO refactor this to remove duplication (also in framebuffer.rs)
pub fn blit_user(
    addr: crate::arch::mem::VirtAddr,
) -> BlitManager<
    Simplegraphics<'static, RawFrameBuffer>,
    Simplegraphics<'static, GlobalFrameBuffer>,
    RawFrameBuffer,
> {
    let intermediate = Simplegraphics::new(Box::leak(Box::new(unsafe {
        RawFrameBuffer::new_user(
            addr,
            GLOBAL_FRAMEBUFFER.width(),
            GLOBAL_FRAMEBUFFER.height(),
            GLOBAL_FRAMEBUFFER.bpp(),
        )
    })));
    let target = Simplegraphics::new(&*GLOBAL_FRAMEBUFFER);
    BlitManager::new(intermediate, target)
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

pub trait GFXManager: Debug + Send + Sync {
    fn draw_primitive(&self, primitive: &PrimitiveGlyph) -> Result<(), GraphicsError>;
    fn draw_batched_primitives(&self, primitives: &[&PrimitiveGlyph]) -> Result<(), GraphicsError>;
    fn draw_glyph(&self, glyph: &dyn Glyph, color: &ColorCode);
    fn flush(&self, area: &BoundingBox) {}
}

impl<B, T, F> FileRepr for BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
    fn fstat(&self) -> crate::kernel::fd::FStat {
        crate::kernel::fd::FStat::new()
    }

    fn node_type(&self) -> crate::kernel::fs::NodeType {
        crate::kernel::fs::NodeType::Dir
    }
}

impl<B, T, F> IOCapable for BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
}

impl<B, T, F> Read for BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        Err(IOError::simple(
            crate::kernel::fs::FSErrorKind::NotSupported,
        ))
    }
}

impl<B, T, F> Write for BlitManager<B, T, F>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + HasFrameBuffer<F>,
    F: FrameBuffer,
    T: BlitTarget,
{
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        // using buf as a mem region of Bounding Boxes and offset as the length of the region
        let len = unsafe { *(buf[0..8].as_ptr() as *const usize) };
        let bounds = unsafe {
            &*core::ptr::slice_from_raw_parts(buf[8..].as_ptr() as *const BoundingBox, len)
        };
        for bound in bounds {
            self.flush(bound);
        }

        Ok(len)
    }
}

pub struct SimpleGFXManager<B>
where
    B: PrimitiveDrawTarget + DrawTarget<Color = RGBColor, Error = GraphicsError> + Send,
{
    inner: Mutex<B>,
}

impl<B> SimpleGFXManager<B>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + Send,
{
    fn new(backend: B) -> Self {
        Self {
            inner: Mutex::new(backend),
        }
    }
}

impl<B> GFXManager for SimpleGFXManager<B>
where
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + Send,
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
    B: PrimitiveDrawTarget
        + DrawTarget<Color = RGBColor, Error = GraphicsError>
        + GraphicsBackend
        + Send,
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
        writeln!(f, "BlitManager")
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
