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

pub struct Circle {
    pub center: Point,
    pub rad: usize,
}

impl Glyph for Circle {
    fn render_colorized(&self, color: &ColorCode, gfx: &dyn super::GraphicsBackend) {
        let mut x: isize = self.rad as isize;
        let mut y: isize = 0;
        let mut err: isize = 0;
        let cx = self.center.x as isize;
        let cy = self.center.y as isize;

        while x >= y {
            gfx.draw_pixel(&Point::saturated_add_from_components(cx, x, cy, y), color);
            gfx.draw_pixel(&Point::saturated_add_from_components(cx, y, cy, x), color);
            gfx.draw_pixel(&Point::saturated_add_from_components(cx, -y, cy, x), color);
            gfx.draw_pixel(&Point::saturated_add_from_components(cx, -x, cy, y), color);
            gfx.draw_pixel(&Point::saturated_add_from_components(cx, -x, cy, -y), color);
            gfx.draw_pixel(&Point::saturated_add_from_components(cx, -y, cy, -x), color);
            gfx.draw_pixel(&Point::saturated_add_from_components(cx, y, cy, -x), color);
            gfx.draw_pixel(&Point::saturated_add_from_components(cx, x, cy, -y), color);
            y += 1;
            if err <= 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err += 2 * (y - x) + 1;
            }
        }
    }
}

impl From<embedded_graphics::primitives::Circle> for Circle {
    fn from(value: embedded_graphics::primitives::Circle) -> Self {
        Self {
            center: value.center().into(),
            rad: value.diameter as usize / 2,
        }
    }
}

impl From<&embedded_graphics::primitives::Circle> for Circle {
    fn from(value: &embedded_graphics::primitives::Circle) -> Self {
        Self {
            center: value.center().into(),
            rad: value.diameter as usize / 2,
        }
    }
}

impl From<Circle> for embedded_graphics::primitives::Circle {
    fn from(value: Circle) -> Self {
        let p: embedded_graphics::prelude::Point = value.center.into();
        Self {
            top_left: p - embedded_graphics::prelude::Point {
                x: value.rad as i32,
                y: value.rad as i32,
            },
            diameter: value.rad as u32 * 2,
        }
    }
}

#[derive(Clone)]
pub struct Point {
    pub x: usize,
    pub y: usize,
}

impl Point {
    fn saturated_add_from_components(x0: isize, x1: isize, y0: isize, y1: isize) -> Self {
        Self {
            x: x0.saturating_add(x1).max(0) as usize,
            y: y0.saturating_add(y1).max(0) as usize,
        }
    }
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

impl Add for &Point {
    type Output = Point;
    fn add(self, rhs: Self) -> Self::Output {
        Point {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl From<embedded_graphics::prelude::Point> for Point {
    fn from(value: embedded_graphics::prelude::Point) -> Self {
        Self {
            x: value.x.max(0) as usize,
            y: value.y.max(0) as usize,
        }
    }
}
impl From<&embedded_graphics::prelude::Point> for Point {
    fn from(value: &embedded_graphics::prelude::Point) -> Self {
        Self {
            x: value.x.max(0) as usize,
            y: value.y.max(0) as usize,
        }
    }
}

impl From<Point> for embedded_graphics::prelude::Point {
    fn from(value: Point) -> Self {
        Self {
            x: value.x as i32,
            y: value.y as i32,
        }
    }
}
