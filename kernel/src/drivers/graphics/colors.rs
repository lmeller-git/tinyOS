// r g b
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
            ColorCode::Black => Self(0, 0, 0),
            ColorCode::White => Self(u8::MAX, u8::MAX, u8::MAX),
            ColorCode::Red => Self(u8::MAX, 0, 0),
            ColorCode::Green => Self(0, u8::MAX, 0),
            ColorCode::Blue => Self(0, 0, u8::MAX),
        }
    }
}

#[derive(Default)]
pub enum ColorCode {
    #[default]
    Black,
    White,
    Red,
    Green,
    Blue,
}
