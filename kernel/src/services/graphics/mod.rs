use shapes::{Line, Point};

use crate::drivers::graphics::{colors::ColorCode, framebuffers::FrameBuffer};

pub mod shapes;
pub mod text;

pub trait GraphicsBackend {
    fn draw_pixel(&self, p: &Point, color: &ColorCode);
    fn draw_line(&self, start: &Point, end: &Point, color: &ColorCode);
    fn width(&self) -> usize;
    fn height(&self) -> usize;
}

pub struct Simplegraphics<'a> {
    fb: &'a dyn FrameBuffer,
}

impl<'a> Simplegraphics<'a> {
    pub fn new(fb: &'a dyn FrameBuffer) -> Self {
        Self { fb }
    }
}

impl GraphicsBackend for Simplegraphics<'_> {
    fn draw_pixel(&self, p: &Point, color: &ColorCode) {
        self.fb.set_pixel(&color.into(), p.x, p.y);
    }
    fn draw_line(&self, start: &Point, end: &Point, color: &ColorCode) {
        // TODO optimizei
        //
        // naive:

        // let mut delta = end.clone() - start.clone();
        // let l = Line {
        //     start: start.clone(),
        //     end: end.clone(),
        // }
        // .len();
        // delta.x /= l;
        // delta.y /= l;
        // let c = color.into();
        // for i in 0..l {
        //     self.fb
        //         .set_pixel(&c, start.x + i * delta.x, start.y + i * delta.y);
        // }

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
        self.fb.width()
    }
}

pub trait Glyph {
    fn render(&self, gfx: &dyn GraphicsBackend) {
        self.render_colorized(&ColorCode::default(), gfx);
    }
    fn render_colorized(&self, color: &ColorCode, gfx: &dyn GraphicsBackend);
}
