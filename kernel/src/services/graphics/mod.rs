use core::fmt::Debug;

use alloc::{boxed::Box, vec::Vec};
use embedded_graphics::{
    mono_font::MonoTextStyle,
    prelude::{DrawTarget, OriginDimensions},
    primitives::{self, StyledDrawable},
    text::renderer::TextRenderer,
};
use shapes::Point;
use thiserror::Error;

use crate::drivers::graphics::{
    colors::{ColorCode, RGBColor},
    framebuffers::{BoundingBox, FrameBuffer, HasFrameBuffer},
};

pub mod shapes;
pub mod text;

pub trait PrimitiveDrawTarget: Debug {
    fn draw_primitive(&mut self, item: &PrimitiveGlyph<'_>) -> Result<(), GraphicsError>;
    fn clear(&mut self, color: RGBColor) -> Result<(), GraphicsError>;
}

pub trait BlitTarget {
    unsafe fn copy_row(&self, from: *const u32, len: usize, x: usize, y: usize);
    fn copy_rect<F: FrameBuffer>(&self, area: &BoundingBox, buf: &F);
}

pub enum PrimitiveGlyph<'a> {
    Circle(primitives::Circle, primitives::PrimitiveStyle<RGBColor>),
    Rect(primitives::Rectangle, primitives::PrimitiveStyle<RGBColor>),
    Arc(primitives::Arc, primitives::PrimitiveStyle<RGBColor>),
    RoundedRect(
        primitives::RoundedRectangle,
        primitives::PrimitiveStyle<RGBColor>,
    ),
    Ellipse(primitives::Ellipse, primitives::PrimitiveStyle<RGBColor>),
    Line(primitives::Line, primitives::PrimitiveStyle<RGBColor>),
    Polyline(
        primitives::Polyline<'a>,
        primitives::PrimitiveStyle<RGBColor>,
    ),
    Triangle(primitives::Triangle, primitives::PrimitiveStyle<RGBColor>),
    ContiguousFilling(primitives::Rectangle, Vec<RGBColor>),
    SolidFilling(primitives::Rectangle, RGBColor),
    Text(
        &'a MonoTextStyle<'a, RGBColor>,
        &'a str,
        embedded_graphics::prelude::Point,
    ),
}

impl PrimitiveGlyph<'_> {
    pub fn render_in<D>(&self, target: &mut D) -> Result<(), GraphicsError>
    where
        D: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        match self {
            Self::Circle(shape, style) => shape.draw_styled(style, target),
            Self::Rect(shape, style) => shape.draw_styled(style, target),
            Self::RoundedRect(shape, style) => shape.draw_styled(style, target),
            Self::Arc(shape, style) => shape.draw_styled(style, target),
            Self::Ellipse(shape, style) => shape.draw_styled(style, target),
            Self::Line(shape, style) => shape.draw_styled(style, target),
            Self::Polyline(shape, style) => shape.draw_styled(style, target),
            Self::Triangle(shape, style) => shape.draw_styled(style, target),
            Self::ContiguousFilling(shape, colors) => target.fill_contiguous(shape, colors.clone()),
            Self::SolidFilling(shape, color) => target.fill_solid(shape, *color),
            Self::Text(style, text, position) => style
                .draw_string(
                    text,
                    *position,
                    embedded_graphics::text::Baseline::Alphabetic,
                    target,
                )
                .map(|_| ()),
        }
    }
}

impl<T> PrimitiveDrawTarget for T
where
    T: Debug + DrawTarget<Color = RGBColor, Error = GraphicsError> + GraphicsBackend,
{
    fn draw_primitive(&mut self, item: &PrimitiveGlyph<'_>) -> Result<(), GraphicsError> {
        item.render_in(self)
    }

    fn clear(&mut self, color: RGBColor) -> Result<(), GraphicsError> {
        DrawTarget::clear(self, color)
    }
}

pub trait GraphicsBackend {
    fn draw_pixel(&self, p: &Point, color: &ColorCode) {}
    fn draw_line(&self, start: &Point, end: &Point, color: &ColorCode) {}
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
