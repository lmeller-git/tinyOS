use embedded_graphics::{
    pixelcolor::raw::RawU32,
    prelude::{PixelColor, RgbColor},
};

// r g b
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct RGBColor(pub u8, pub u8, pub u8);

impl RGBColor {}

impl Default for RGBColor {
    fn default() -> Self {
        Self(0, 0, 0)
    }
}

impl From<ColorCode> for RGBColor {
    fn from(value: ColorCode) -> Self {
        Self::from(&value)
    }
}

impl From<&ColorCode> for RGBColor {
    fn from(value: &ColorCode) -> Self {
        match value {
            ColorCode::RGBColor(r, g, b) => Self(*r, *g, *b),
            ColorCode::Black => Self(0, 0, 0),
            ColorCode::White => Self(u8::MAX, u8::MAX, u8::MAX),
            ColorCode::Red => Self(u8::MAX, 0, 0),
            ColorCode::Green => Self(0, u8::MAX, 0),
            ColorCode::Blue => Self(0, 0, u8::MAX),
            ColorCode::Yellow => Self(u8::MAX, u8::MAX, 0),
            ColorCode::Cyan => Self(0, u8::MAX, u8::MAX),
            ColorCode::Magenta => Self(u8::MAX, 0, u8::MAX),
            ColorCode::Gray => Self(128, 128, 128),
            ColorCode::LightGray => Self(192, 192, 192),
            ColorCode::DarkGray => Self(64, 64, 64),
            ColorCode::Orange => Self(255, 165, 0),
            ColorCode::Pink => Self(255, 192, 203),
            ColorCode::Purple => Self(128, 0, 128),
            ColorCode::Brown => Self(139, 69, 19),
        }
    }
}

impl PixelColor for RGBColor {
    type Raw = RawU32;
}

impl RgbColor for RGBColor {
    const MAX_R: u8 = u8::MAX;
    const MAX_G: u8 = u8::MAX;
    const MAX_B: u8 = u8::MAX;
    const BLACK: Self = Self(0, 0, 0);
    const RED: Self = Self(Self::MAX_R, 0, 0);
    const GREEN: Self = Self(0, Self::MAX_G, 0);
    const BLUE: Self = Self(0, 0, Self::MAX_B);
    const YELLOW: Self = Self(Self::MAX_R, Self::MAX_G, 0);
    const MAGENTA: Self = Self(Self::MAX_R, 0, Self::MAX_B);
    const CYAN: Self = Self(0, Self::MAX_G, Self::MAX_B);
    const WHITE: Self = Self(Self::MAX_R, Self::MAX_G, Self::MAX_B);

    fn r(&self) -> u8 {
        self.0
    }
    fn g(&self) -> u8 {
        self.1
    }
    fn b(&self) -> u8 {
        self.2
    }
}

#[derive(Default)]
pub enum ColorCode {
    RGBColor(u8, u8, u8),
    #[default]
    Black,
    White,
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    Gray,
    LightGray,
    DarkGray,
    Orange,
    Pink,
    Purple,
    Brown,
}
