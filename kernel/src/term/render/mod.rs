#![allow(dead_code, unused_variables)]
#![cfg_attr(feature = "test_run", allow(static_mut_refs))]

use crate::{
    drivers::graphics::{
        colors::{ColorCode, RGBColor},
        text::CharRenderer,
    },
    locks::thread_safe::Mutex,
    services::graphics::GraphicsError,
};
use core::{
    fmt::{Debug, Write},
    ops::{Add, Range},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, MonoTextStyleBuilder, ascii},
    prelude::{DrawTarget, Point, Size},
    primitives::Rectangle,
    text::Baseline,
};
use os_macros::{kernel_test, tests};
use thiserror::Error;

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
            max_row: ybounds.into(),
            max_col: xbounds.into(),
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
            x: value.col.as_ipixel(CHAR_WIDTH),
            y: value.row.as_ipixel(CHAR_HEIGHT),
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
pub(super) struct TermCharBuffer<const X: usize, const Y: usize> {
    inner: [[Option<char>; X]; Y],
}

impl<const X: usize, const Y: usize> TermCharBuffer<X, Y> {
    pub(super) const fn new() -> Self {
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

    fn shift_up_and_redraw<B>(&mut self, gfx: &mut B, style: &MonoTextStyle<'_, RGBColor>)
    where
        B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        for row in 0..Y - 1 {
            let pixel = TermPixel { inner: row };
            self.redraw_empty_row(&pixel, gfx);
            self.inner[row] = self.inner[row + 1];
            self.redraw_row_with_range(&pixel, gfx, style, self.get_range_from_row(&pixel));
        }
        self.clear_line(&TermPixel { inner: Y - 1 });
        self.redraw_empty_row(&TermPixel { inner: Y - 1 }, gfx);
        // self.shift_up();
        // self.redraw(&mut TermPosition::new(0, 0, X, Y), gfx, style);
    }

    fn shift_down(&mut self) {
        for row in (1..Y).rev() {
            self.inner[row] = self.inner[row - 1];
        }
        self.clear_line(&TermPixel { inner: 0 });
    }

    fn get_range_from_row(&self, row: &TermPixel) -> Range<usize> {
        // assuming no gaps
        for (i, item) in self.inner[row.inner].iter().enumerate() {
            if item.is_none() {
                return 0..i;
            }
        }
        0..X
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

    fn redraw_empty_row<B>(&mut self, row: &TermPixel, gfx: &mut B)
    where
        B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        _ = gfx.fill_solid(
            &embedded_graphics::primitives::Rectangle {
                top_left: Point::new(0, row.as_ipixel(CHAR_HEIGHT)),
                size: Size::new(gfx.bounding_box().size.width, CHAR_HEIGHT as u32),
            },
            ColorCode::default().into(),
        );
    }

    fn redraw_row_with_range<B>(
        &mut self,
        row: &TermPixel,
        gfx: &mut B,
        style: &MonoTextStyle<'_, RGBColor>,
        range: Range<usize>,
    ) where
        B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        for col in range {
            _ = style.draw_char(
                self.inner[row.inner][col].unwrap_or(' '),
                Point::new(
                    TermPixel { inner: col }.as_ipixel(CHAR_WIDTH),
                    row.as_ipixel(CHAR_HEIGHT),
                ),
                Baseline::Top,
                gfx,
            );
        }
    }

    fn redraw_col<B>(&mut self, col: &TermPixel, gfx: &mut B)
    where
        B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        todo!()
    }

    fn redraw<B>(&self, cursor: &mut TermPosition, gfx: &mut B, style: &MonoTextStyle<'_, RGBColor>)
    where
        B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
    {
        // This method is EXTREMELY inefficient, as it redraws everything. Use only if no other option
        _ = gfx.clear(ColorCode::default().into());
        let current = *cursor;
        cursor.row.inner = 0;
        for y in 0..Y {
            cursor.row.inner = y;
            for x in 0..X {
                cursor.col.inner = x;
                match self.inner[y][x] {
                    None => {
                        // assuming all cols up to the last filled one are filled, ie ch, ch, None, None, None, ...
                        // this simply optimizes the loop slightly
                        break;
                    }
                    Some(c) => {
                        _ = style.draw_char(c, (*cursor).into(), Baseline::Top, gfx);
                    }
                }
            }
        }
        *cursor = current;
    }

    fn force_push_smart(
        &mut self,
        ch: char,
        cursor: &mut TermPosition,
    ) -> Result<(), PositionError> {
        let mut should_redraw = false;
        let mut should_redraw_all = false;
        if cursor.col.inner >= X {
            // self.shift_up();
            cursor.col.inner = 0;
            cursor.row.inner += 1;
            should_redraw = true;
        }
        while cursor.row.inner >= Y {
            self.shift_up();
            cursor.row.inner -= 1;
            should_redraw_all = true;
        }

        self.inner[cursor.row.inner][cursor.col.inner].replace(ch);
        if should_redraw && !should_redraw_all {
            // cursor.col.inner += 1;
            Err(PositionError::NewLine)
        } else if should_redraw_all {
            Err(PositionError::PrevLine)
        } else {
            // cursor.col.inner += 1;
            Ok(())
        }
    }

    fn push_dumb(&mut self, ch: char, cursor: &TermPosition) -> Result<(), PositionError> {
        self.get_mut(cursor).map(|item| {
            item.replace(ch);
        })
    }

    fn is_empty(&self) -> bool {
        !self
            .inner
            .iter()
            .any(|row| row.iter().any(|item| item.is_some()))
    }
}

pub struct BasicTermRender<'a, B, const X: usize, const Y: usize>
where
    B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
{
    backend: &'a Mutex<B>,
    cursor: TermPosition,
    str_style: MonoTextStyle<'a, RGBColor>,
    buffer: &'a mut TermCharBuffer<X, Y>,
}

impl<'a, B, const X: usize, const Y: usize> BasicTermRender<'a, B, X, Y>
where
    B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
{
    pub(super) fn new(gfx: &'a Mutex<B>, buffer: &'a mut TermCharBuffer<X, Y>) -> Self {
        let bounds = { gfx.lock().bounding_box() };
        // MAX_CHARS_X and MAX_CHARS_Y :
        // serial_println!(
        //     "w: {}, h: {}",
        //     (bounds.size.width as usize) / CHAR_WIDTH,
        //     (bounds.size.height as usize) / CHAR_HEIGHT
        // );
        Self {
            backend: gfx,
            cursor: TermPosition::new(
                0,
                0,
                (bounds.size.width as usize) / CHAR_WIDTH, // x
                (bounds.size.height as usize) / CHAR_HEIGHT, // y
            ),
            str_style: MonoTextStyleBuilder::new()
                .font(&ascii::FONT_10X20)
                .background_color(ColorCode::Black.into())
                .text_color(ColorCode::White.into())
                .build(),
            buffer,
        }
    }

    pub(super) fn line_clear(&mut self) {
        self.buffer.clear_line(&self.cursor.row);
        self.buffer
            .redraw_empty_row(&self.cursor.row, &mut *self.backend.lock());
    }

    pub(super) fn clear_one(&mut self) {
        loop {
            if self.cursor.col.inner > 0 {
                self.cursor.col.inner -= 1;
            } else if self.cursor.row.inner > 0 {
                self.cursor.row.inner -= 1;
                self.cursor.col.inner = X - 1;
            } else {
                break;
            }
            if self.buffer.get(&self.cursor).unwrap().is_some() {
                break;
            }
        }

        self.backend.lock().fill_solid(
            &Rectangle::new(
                self.cursor.into(),
                Size::new(CHAR_WIDTH as u32, CHAR_HEIGHT as u32),
            ),
            ColorCode::default().into(),
        );
    }

    fn write_tab(&mut self) {
        // tab == 3 spaces TODO add dynamic tab
        for _ in 0..3 {
            self.write_char(' ');
        }
    }

    fn write_char(&mut self, c: char) {
        match c {
            '\n' => {
                // This will try to draw /n, which is ?
                // _ = self.buffer.push_dumb(c, &self.cursor);
                self.newline();
            }
            '\t' => self.write_tab(),
            '\r' => self.line_clear(),
            _ => {
                // self.cleanup(res);
                // serial_println!("c: {:#?}", self.cursor);
                match self.buffer.force_push_smart(c, &mut self.cursor) {
                    Err(PositionError::NewLine) => {
                        // serial_println!("e1");
                        self.buffer.redraw_row_with_range(
                            &self.cursor.row,
                            &mut *self.backend.lock(),
                            &self.str_style,
                            self.buffer.get_range_from_row(&self.cursor.row),
                        );
                        self.cursor.col.inner += 1;
                    }
                    Err(PositionError::PrevLine) => {
                        // serial_println!("e2");
                        self.buffer.redraw(
                            &mut self.cursor,
                            &mut *self.backend.lock(),
                            &self.str_style,
                        );
                        // self.buffer
                        // .redraw_empty_row(&self.cursor.row, &mut *self.backend.lock());
                        self.cursor.col.inner += 1;
                    }
                    Ok(()) => {
                        // serial_println!("ok");
                        let res = self.str_style.draw_char(
                            c,
                            self.cursor.into(),
                            Baseline::Top,
                            &mut *self.backend.lock(),
                        );
                        self.cursor.col.inner += 1;
                    }
                    _ => {
                        // serial_println!("e3");
                    }
                };
            }
        }
    }

    fn write_char_iter(&mut self, chars: impl Iterator<Item = char>) {
        for c in chars {
            self.write_char(c);
        }
    }

    #[allow(unreachable_code)]
    fn cleanup(&mut self, draw_res: Result<Point, GraphicsError>) {
        // TODO
        todo!();
        return;
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
        if self.cursor.row.inner >= Y - 1 {
            self.buffer
                .shift_up_and_redraw(&mut *self.backend.lock(), &self.str_style);
        } else {
            self.cursor.row.inner += 1;
        }
        self.cursor.col = 0.into();
    }

    pub(super) fn prevline(&mut self) {
        // TODO
        // shifts all content down by one line
        self.buffer.shift_down();
        self.buffer
            .redraw(&mut self.cursor, &mut *self.backend.lock(), &self.str_style);
    }
}

impl<B, const X: usize, const Y: usize> Write for BasicTermRender<'_, B, X, Y>
where
    B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_char_iter(s.chars());
        Ok(())
    }
}

impl<B, const X: usize, const Y: usize> Debug for BasicTermRender<'_, B, X, Y>
where
    B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "X: {}, Y: {}", X, Y)?;
        writeln!(f, "{:#?}", self.cursor)?;
        Ok(())
    }
}

mod tests {
    use super::*;
    #[kernel_test]
    fn print_to_buffer() {
        // SAFETY This is safe, as long it is not run parallely to some other functionality accessing FOOBAR / BAR, and init_term() was run in the same execution context
        use crate::{print, println};
        unsafe { super::super::BAR.clear() };
        unsafe { assert!(super::super::BAR.is_empty()) };
        unsafe {
            super::super::FOOBAR.get_unchecked().lock().cursor.row.inner = 0;
            super::super::FOOBAR.get_unchecked().lock().cursor.col.inner = 0;
        };
        println!("test");
        let mut row = [None; super::super::MAX_CHARS_X];
        row[0].replace('t');
        row[1].replace('e');
        row[2].replace('s');
        row[3].replace('t');
        unsafe { assert_eq!(row, super::super::BAR.inner[0]) };
        unsafe {
            assert_eq!(
                super::super::FOOBAR.get_unchecked().lock().cursor.row,
                TermPixel { inner: 1 }
            )
        };
        unsafe {
            assert_eq!(
                super::super::FOOBAR.get_unchecked().lock().cursor.col,
                TermPixel { inner: 0 }
            )
        };
        print!("test2");
        unsafe { assert_eq!(row, super::super::BAR.inner[0]) };
        row[4].replace('2');
        unsafe { assert_eq!(row, super::super::BAR.inner[1]) };
        unsafe {
            assert_eq!(
                super::super::FOOBAR.get_unchecked().lock().cursor.row,
                TermPixel { inner: 1 }
            )
        };
        unsafe {
            assert_eq!(
                super::super::FOOBAR.get_unchecked().lock().cursor.col,
                TermPixel { inner: 5 }
            )
        };
        print!("hey");
        row[5].replace('h');
        row[6].replace('e');
        row[7].replace('y');
        unsafe { assert_eq!(row, super::super::BAR.inner[1]) };
        unsafe {
            assert_eq!(
                super::super::FOOBAR.get_unchecked().lock().cursor.row,
                TermPixel { inner: 1 }
            )
        };
        unsafe {
            assert_eq!(
                super::super::FOOBAR.get_unchecked().lock().cursor.col,
                TermPixel { inner: 8 }
            )
        };
    }

    #[kernel_test]
    fn buf_shifts() {
        // SAFETY This is safe, as long it is not run parallely to some other functionality accessing FOOBAR / BAR, and init_term() was run in the same execution context
        use crate::println;
        unsafe { super::super::BAR.clear() };
        unsafe { assert!(super::super::BAR.is_empty()) };
        unsafe {
            super::super::FOOBAR.get_unchecked().lock().cursor.row.inner = 0;
            super::super::FOOBAR.get_unchecked().lock().cursor.col.inner = 0;
        };

        println!();
        println!("test");
        println!("42");
        println!("world");

        unsafe { super::super::BAR.shift_up() };

        let mut row = [None; super::super::MAX_CHARS_X];
        row[0].replace('t');
        row[1].replace('e');
        row[2].replace('s');
        row[3].replace('t');

        unsafe { assert_eq!(row, super::super::BAR.inner[0]) };
        unsafe { super::super::BAR.shift_up() };

        row[0].replace('4');
        row[1].replace('2');
        row[2] = None;
        row[3] = None;

        unsafe { assert_eq!(row, super::super::BAR.inner[0]) };
        unsafe { super::super::BAR.shift_up() };

        row[0].replace('w');
        row[1].replace('o');
        row[2].replace('r');
        row[3].replace('l');
        row[4].replace('d');

        unsafe { assert_eq!(row, super::super::BAR.inner[0]) };
        unsafe { super::super::BAR.shift_up() };
        unsafe { assert!(super::super::BAR.is_empty()) };
        //TODO also test/implement shift_down
    }

    #[kernel_test]
    fn print_many() {
        use crate::print;
        unsafe { super::super::BAR.clear() };
        for _ in 0..300 {
            print!(".");
        }
    }
}
