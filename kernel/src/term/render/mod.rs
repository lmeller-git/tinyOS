use core::{
    fmt::Write,
    ops::{Add, Index},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, MonoTextStyleBuilder, ascii},
    prelude::{DrawTarget, DrawTargetExt, Point},
    text::{Baseline, renderer::TextRenderer},
};
use spin::Mutex;
use thiserror::Error;

use crate::{
    drivers::graphics::{
        colors::{ColorCode, RGBColor},
        text::{CharRenderer, draw_str},
    },
    serial_println,
    services::graphics::{GraphicsBackend, GraphicsError},
};

mod layout;
mod text;

const CHAR_WIDTH: usize = 10;
const CHAR_HEIGHT: usize = 20;

#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[repr(transparent)]
struct TermPixel {
    inner: usize,
}

impl TermPixel {
    fn as_ipixel(&self, multiplier: usize) -> i32 {
        (self.inner * multiplier) as i32
    }

    fn from_ipixel(value: i32, multiplier: usize) -> Self {
        Self {
            inner: value as usize / multiplier,
        }
    }
}

impl Add for TermPixel {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            inner: self.inner + rhs.inner,
        }
    }
}

impl From<usize> for TermPixel {
    fn from(value: usize) -> Self {
        Self { inner: value }
    }
}

impl From<TermPixel> for i32 {
    fn from(value: TermPixel) -> Self {
        value.inner as i32
    }
}

// The position in the terminal. This serves as index for TermCharBuffer
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
struct TermPosition {
    row: TermPixel,
    col: TermPixel,
    max_row: TermPixel,
    max_col: TermPixel,
}

impl TermPosition {
    fn new(row: usize, col: usize, xbounds: usize, ybounds: usize) -> Self {
        Self {
            row: row.into(),
            col: col.into(),
            max_row: xbounds.into(),
            max_col: ybounds.into(),
        }
    }

    fn add_checked(&mut self, dx: usize, dy: usize) -> Result<(), PositionError> {
        todo!()
    }

    fn sub_checked(&mut self, dx: usize, dy: usize) -> Result<(), PositionError> {
        todo!()
    }

    fn shift_checked(&mut self, delta: Point) -> Result<(), PositionError> {
        let mut row = TermPixel::from_ipixel(delta.x, CHAR_WIDTH);
        let mut col = TermPixel::from_ipixel(delta.y, CHAR_HEIGHT);
        if col >= self.max_col {
            col = 0.into();
            if row + 1.into() < self.max_row {
                row = row + 1.into();
            } else {
                self.row = row;
                self.col = col;
                return Err(PositionError::NewLine);
            }
        }
        self.row = row;
        self.col = col;
        Ok(())
    }
}

impl From<TermPosition> for Point {
    fn from(value: TermPosition) -> Self {
        Self {
            x: value.row.as_ipixel(CHAR_HEIGHT),
            y: value.col.as_ipixel(CHAR_WIDTH),
        }
    }
}

#[derive(Error, Debug, PartialEq, Eq, Clone)]
enum PositionError {
    #[error("positon shifted to new line below")]
    NewLine,
    #[error("position shifted to previous line")]
    PrevLine,
    #[error("position is out of bounds: {0:?}")]
    OutOfBounds(TermPosition),
}

#[derive(Debug)]
struct TermCharBuffer<const X: usize, const Y: usize> {
    inner: [[Option<char>; X]; Y],
}

impl<const X: usize, const Y: usize> TermCharBuffer<X, Y> {
    fn new() -> Self {
        Self {
            inner: [[None; X]; Y],
        }
    }

    fn get(&self, index: &TermPosition) -> Result<Option<&char>, PositionError> {
        let x: usize = index.col.inner;
        let y: usize = index.row.inner;
        if y >= Y || x >= X {
            return Err(PositionError::OutOfBounds(*index));
        }
        Ok(self.inner[y][x].as_ref())
    }

    fn get_mut(&mut self, index: &TermPosition) -> Result<&mut Option<char>, PositionError> {
        let x: usize = index.col.inner;
        let y: usize = index.row.inner;
        if y >= Y || x >= X {
            return Err(PositionError::OutOfBounds(*index));
        }
        Ok(&mut self.inner[y][x])
    }

    fn shift_up(&mut self) {
        for row in 0..Y - 1 {
            self.inner[row] = self.inner[row + 1];
        }
        self.clear_line(&TermPixel { inner: Y - 1 });
    }

    fn shift_down(&mut self) {
        for row in (1..Y).rev() {
            self.inner[row] = self.inner[row - 1];
        }
        self.clear_line(&TermPixel { inner: 0 });
    }

    fn clear(&mut self) {
        self.inner = [[None; X]; Y];
    }

    fn clear_line(&mut self, line: &TermPixel) {
        self.inner[line.inner] = [None; X];
    }

    fn clear_col(&mut self, col: &TermPixel) {
        for r in 0..Y {
            self.inner[r][col.inner] = None;
        }
    }

    fn redraw_row<B>(&mut self, row: &TermPixel, gfx: &mut B)
    where
        B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        todo!()
    }

    fn redraw_col<B>(&mut self, col: &TermPixel, gfx: &mut B)
    where
        B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        todo!()
    }

    fn redraw<B>(&self, cursor: &mut TermPosition, gfx: &mut B)
    where
        B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        todo!()
    }
}

pub struct BasicTermRender<'a, B, const X: usize, const Y: usize>
where
    B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
{
    backend: &'a Mutex<B>,
    cursor: TermPosition,
    str_style: MonoTextStyle<'a, RGBColor>,
    buffer: TermCharBuffer<X, Y>,
}

impl<'a, B, const X: usize, const Y: usize> BasicTermRender<'a, B, X, Y>
where
    B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
{
    pub(super) fn new(gfx: &'a Mutex<B>) -> Self {
        let bounds = { gfx.lock().bounding_box() };

        Self {
            backend: gfx,
            cursor: TermPosition::new(
                0,
                0,
                (bounds.size.width as usize) / CHAR_WIDTH,
                (bounds.size.height as usize) / CHAR_HEIGHT,
            ),
            str_style: MonoTextStyleBuilder::new()
                .font(&ascii::FONT_10X20)
                .background_color(ColorCode::Black.into())
                .text_color(ColorCode::White.into())
                .build(),
            buffer: TermCharBuffer::new(),
        }
    }

    pub(super) fn line_clear(&mut self) {
        self.buffer.clear_line(&self.cursor.row);
        self.buffer
            .redraw_row(&self.cursor.row, &mut *self.backend.lock());
    }

    pub(super) fn clear_one(&mut self) {}

    fn write_char(&mut self, c: char) {
        let res = self.str_style.draw_char(
            c,
            self.cursor.into(),
            Baseline::Top,
            &mut *self.backend.lock(),
        );
        self.cleanup(res);
    }

    fn cleanup(&mut self, draw_res: Result<Point, GraphicsError>) {
        match draw_res {
            Ok(p) => match self.cursor.shift_checked(p) {
                Ok(()) => {}
                Err(e) => match e {
                    PositionError::NewLine => self.newline(),
                    PositionError::PrevLine => self.prevline(),
                    _ => {}
                },
            },
            Err(e) => {}
        }
    }

    pub(super) fn newline(&mut self) {
        self.buffer.shift_up();
        // self.cursor.row = ;
        self.cursor.col = 0.into();
        self.buffer
            .redraw(&mut self.cursor, &mut *self.backend.lock());
    }

    pub(super) fn prevline(&mut self) {
        // shifts all content down by one line
        self.buffer.shift_down();
        // self.cursor.col =
        self.buffer
            .redraw(&mut self.cursor, &mut *self.backend.lock());
    }
}

impl<B, const X: usize, const Y: usize> Write for BasicTermRender<'_, B, X, Y>
where
    B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let res = self.str_style.draw_string(
            s,
            self.cursor.into(),
            Baseline::Top,
            &mut *self.backend.lock(),
        );
        self.cleanup(res);
        Ok(())
    }
}
