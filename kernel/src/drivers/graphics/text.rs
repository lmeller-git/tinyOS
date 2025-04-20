use alloc::format;
use embedded_graphics::{
    Drawable,
    image::Image,
    mono_font::MonoTextStyle,
    prelude::{DrawTarget, PixelColor, Point},
    text::{Baseline, renderer::TextRenderer},
};

use crate::services::graphics::GraphicsError;

use super::colors::{ColorCode, RGBColor};

pub fn draw_str<T: DrawTarget<Color = RGBColor, Error = GraphicsError>>(
    s: &str,
    pos: Point,
    gfx: &mut T,
) -> Result<Point, GraphicsError> {
    embedded_graphics::mono_font::MonoTextStyle::new(
        &embedded_graphics::mono_font::ascii::FONT_10X20,
        ColorCode::White.into(),
    )
    .draw_string(s, pos, embedded_graphics::text::Baseline::Alphabetic, gfx)
}

pub trait CharRenderer<C>
where
    C: PixelColor,
{
    fn draw_char<D>(
        &self,
        char: char,
        position: Point,
        baseline: Baseline,
        target: &mut D,
    ) -> Result<Point, GraphicsError>
    where
        D: DrawTarget<Color = C, Error = GraphicsError>;
}

impl<C> CharRenderer<C> for MonoTextStyle<'_, C>
where
    C: PixelColor,
{
    fn draw_char<D>(
        &self,
        char: char,
        position: Point,
        baseline: Baseline,
        target: &mut D,
    ) -> Result<Point, GraphicsError>
    where
        D: DrawTarget<Color = C, Error = GraphicsError>,
    {
        // TODO
        // let position = position - Point::new(0, self.baseline_offset(baseline));
        // let glyph = self.font.glyph(char);
        // Image::new(&glyph, position).draw(target);
        let mut buf = [0u8; 4];
        let s: &str = char.encode_utf8(&mut buf);
        self.draw_string(s, position, baseline, target)
    }
}
