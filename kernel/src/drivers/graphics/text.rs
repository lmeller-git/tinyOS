use embedded_graphics::{
    prelude::{DrawTarget, Point},
    text::renderer::TextRenderer,
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
