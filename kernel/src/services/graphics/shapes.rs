use core::ops::{Add, Sub};

use crate::drivers::graphics::colors::ColorCode;

use super::Glyph;

pub struct Line {
    pub start: Point,
    pub end: Point,
}

impl Line {
    pub fn len(&self) -> usize {
        (((self.end.x - self.start.x) << 2) + ((self.end.y - self.start.y) << 2)).isqrt()
    }
}

impl Glyph for Line {
    fn render_colorized(&self, color: &ColorCode, gfx: &dyn super::GraphicsBackend) {
        gfx.draw_line(&self.start, &self.end, color);
    }
}

pub struct Rect {
    pub top_left: Point,
    pub bottom_right: Point,
}

impl Glyph for Rect {
    fn render_colorized(&self, color: &ColorCode, gfx: &dyn super::GraphicsBackend) {
        let top_right = Point {
            x: self.bottom_right.x,
            y: self.top_left.y,
        };
        let bottom_left = Point {
            x: self.top_left.x,
            y: self.bottom_right.y,
        };
        gfx.draw_line(&self.top_left, &top_right, color);
        gfx.draw_line(&self.top_left, &bottom_left, color);
        gfx.draw_line(&top_right, &self.bottom_right, color);
        gfx.draw_line(&bottom_left, &self.bottom_right, color);
    }
}

#[derive(Clone)]
pub struct Point {
    pub x: usize,
    pub y: usize,
}

impl Glyph for Point {
    fn render_colorized(&self, color: &ColorCode, gfx: &dyn super::GraphicsBackend) {
        gfx.draw_pixel(self, color);
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}
