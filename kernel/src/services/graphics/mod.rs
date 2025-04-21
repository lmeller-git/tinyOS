use embedded_graphics::prelude::{DrawTarget, OriginDimensions};
use shapes::{Line, Point};
use thiserror::Error;

use crate::drivers::graphics::{
    colors::{ColorCode, RGBColor},
    framebuffers::FrameBuffer,
};

pub mod shapes;
pub mod text;

pub trait GraphicsBackend {
    fn draw_pixel(&self, p: &Point, color: &ColorCode);
    fn draw_line(&self, start: &Point, end: &Point, color: &ColorCode);
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn flush(&self) {}
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

impl<B> GraphicsBackend for Simplegraphics<'_, B>
where
    B: FrameBuffer,
{
    fn draw_pixel(&self, p: &Point, color: &ColorCode) {
        self.fb.set_pixel(&color.into(), p.x, p.y);
    }
    fn draw_line(&self, start: &Point, end: &Point, color: &ColorCode) {
        // TODO optimize
        // bresenham:

        let color = color.into();

        let mut x0 = start.x as isize;
        let mut y0 = start.y as isize;
        let x1 = end.x as isize;
        let y1 = end.y as isize;

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.fb.set_pixel(&color, x0 as usize, y0 as usize);

            if x0 == x1 && y0 == y1 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
    fn width(&self) -> usize {
        self.fb.width()
    }
    fn height(&self) -> usize {
        self.fb.height()
    }
}

pub trait Glyph {
    fn render(&self, gfx: &dyn GraphicsBackend) {
        self.render_colorized(&ColorCode::default(), gfx);
    }
    fn render_colorized(&self, color: &ColorCode, gfx: &dyn GraphicsBackend);
}

#[derive(Error, Debug)]
pub enum GraphicsError {
    #[error("not implemented")]
    NotImplemented,
}
