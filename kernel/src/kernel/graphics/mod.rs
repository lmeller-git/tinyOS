use core::fmt::Debug;

use embedded_graphics::prelude::{DrawTarget, OriginDimensions};
use framebuffers::GlobalFrameBuffer;
use lazy_static::lazy_static;
use thiserror::Error;

use crate::{
    impl_fb_for_hasfb,
    impl_write_for_fb,
    kernel::graphics::{
        colors::RGBColor,
        framebuffers::{BoundingBox, FrameBuffer, HasFrameBuffer},
    },
};

pub mod colors;
pub mod framebuffers;
pub mod text;

lazy_static! {
    pub static ref GLOBAL_FRAMEBUFFER: GlobalFrameBuffer = GlobalFrameBuffer::new_static();
}

pub trait BlitTarget {
    unsafe fn copy_row(&self, from: *const u32, len: usize, x: usize, y: usize);
    fn copy_rect<F: FrameBuffer>(&self, area: &BoundingBox, buf: &F);
}

pub struct Simplegraphics<'a, B>
where
    B: FrameBuffer,
{
    fb: &'a B,
}

impl<'a, B> Simplegraphics<'a, B>
where
    B: FrameBuffer,
{
    pub fn new(fb: &'a B) -> Self {
        Self { fb }
    }
}

impl<B> Debug for Simplegraphics<'_, B>
where
    B: FrameBuffer,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SimpleGraphics")?;
        Ok(())
    }
}

impl<B> DrawTarget for Simplegraphics<'_, B>
where
    B: FrameBuffer,
{
    type Color = RGBColor;
    type Error = GraphicsError;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for p in pixels {
            self.fb.set_pixel(&p.1, p.0.x as usize, p.0.y as usize);
        }
        Ok(())
    }
}

impl<B> OriginDimensions for Simplegraphics<'_, B>
where
    B: FrameBuffer,
{
    fn size(&self) -> embedded_graphics::prelude::Size {
        embedded_graphics::prelude::Size {
            width: self.width() as u32,
            height: self.height() as u32,
        }
    }
}

impl<B> BlitTarget for Simplegraphics<'_, B>
where
    B: FrameBuffer,
{
    unsafe fn copy_row(&self, from: *const u32, len: usize, x: usize, y: usize) {
        unsafe {
            core::ptr::copy_nonoverlapping(
                from,
                self.fb.addr().add(self.fb.pixel_offset(x, y)).cast::<u32>(),
                len,
            )
        };
    }

    fn copy_rect<F: FrameBuffer>(&self, area: &BoundingBox, buf: &F) {
        assert!(area.width + area.x <= self.fb.width());
        assert!(area.height + area.y <= self.fb.height());
        assert_eq!(buf.bpp(), self.fb.bpp());

        for row in area.y..area.y + area.height {
            unsafe {
                self.copy_row(
                    buf.addr().add(buf.pixel_offset(area.x, row)).cast::<u32>(),
                    area.width,
                    area.x,
                    row,
                )
            };
        }
    }
}

impl<B> HasFrameBuffer<B> for Simplegraphics<'_, B>
where
    B: FrameBuffer,
{
    fn get_framebuffer(&self) -> &B {
        self.fb
    }
}

impl_fb_for_hasfb!(Simplegraphics<'_, B> where [B: FrameBuffer]);
impl_write_for_fb!(Simplegraphics<'_, B> where [B: FrameBuffer]);

#[derive(Error, Debug)]
pub enum GraphicsError {
    #[error("not implemented")]
    NotImplemented,
}
