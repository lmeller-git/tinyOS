use core::fmt;

use lazy_static::lazy_static;
use spin::Mutex;

#[allow(dead_code)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VGAColor {
    #[default]
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

// This is just lazy
pub struct Volatile<T> {
    inner: T,
}

impl<T> Volatile<T> {
    fn update(&mut self, func: impl Fn(&mut T)) {
        func(&mut self.inner);
    }

    fn write(&mut self, value: T) {
        self.inner = value;
    }

    fn read(&self) -> &T {
        &self.inner
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(transparent)]
struct ColorCode(u8);

#[allow(dead_code)]
impl ColorCode {
    fn new(fg: VGAColor, bg: VGAColor) -> Self {
        // fg color is stored in bits 8-11, bg in 12-14
        Self((bg as u8) << 4 | (fg as u8))
    }

    fn fg(&mut self, fg: VGAColor) -> &mut Self {
        self.0 = (self.0 & 0xF0) | fg as u8;
        self
    }

    fn bg(&mut self, bg: VGAColor) -> &mut Self {
        self.0 = (self.0 & 0x0F) | ((bg as u8) << 4);
        self
    }
}

impl Default for ColorCode {
    fn default() -> Self {
        Self((VGAColor::Black as u8) << 4 | (VGAColor::White as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

#[allow(dead_code)]
impl ScreenChar {
    pub fn fg(&mut self, color: VGAColor) -> &mut Self {
        self.color_code.fg(color);
        self
    }

    pub fn bg(&mut self, color: VGAColor) -> &mut Self {
        self.color_code.bg(color);
        self
    }

    pub fn char(&mut self, char: u8) -> &mut Self {
        self.ascii_character = char;
        self
    }

    pub fn get_char(&self) -> u8 {
        self.ascii_character
    }
}
pub struct ScreenRow<'a> {
    chars: &'a mut [Volatile<ScreenChar>; BUFFER_WIDTH],
}

#[allow(dead_code)]
impl<'a> ScreenRow<'a> {
    pub fn fg(&mut self, color: VGAColor) -> &mut Self {
        self.chars.iter_mut().for_each(|c| {
            c.update(|c| {
                c.fg(color);
            })
        });
        self
    }

    pub fn bg(&mut self, color: VGAColor) -> &mut Self {
        self.chars.iter_mut().for_each(|c| {
            c.update(|c| {
                c.bg(color);
            })
        });
        self
    }

    pub fn char(&mut self, char: u8) -> &mut Self {
        self.chars.iter_mut().for_each(|c| {
            c.update(|c| {
                c.char(char);
            })
        });
        self
    }
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

#[repr(transparent)]
struct VGABuffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

impl VGABuffer {
    fn new() -> *mut Self {
        (0xb8000 + crate::bootinfo::get_phys_offset()) as *mut Self
    }

    fn is_valid_row(&self, row: usize) -> bool {
        (0..BUFFER_HEIGHT).contains(&row)
    }

    fn is_valid_col(&self, col: usize) -> bool {
        (0..BUFFER_WIDTH).contains(&col)
    }

    fn get_mut(&mut self, row: usize, col: usize) -> Option<&mut Volatile<ScreenChar>> {
        if self.is_valid_row(row) && self.is_valid_col(col) {
            Some(&mut self.chars[row][col])
        } else {
            None
        }
    }

    fn get(&self, row: usize, col: usize) -> Option<&Volatile<ScreenChar>> {
        if self.is_valid_row(row) && self.is_valid_col(col) {
            Some(&self.chars[row][col])
        } else {
            None
        }
    }

    fn get_row_mut(&mut self, row: usize) -> Option<&mut [Volatile<ScreenChar>; BUFFER_WIDTH]> {
        if self.is_valid_row(row) {
            Some(&mut self.chars[row])
        } else {
            None
        }
    }
}

pub struct VGAWriter {
    column: usize,
    row: usize,
    buffer: &'static mut VGABuffer,
}

#[allow(dead_code)]
impl VGAWriter {
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.shift_up();
            }
            byte => {
                if self.column >= BUFFER_WIDTH {
                    self.shift_up();
                }
                self.write_byte_unchecked(byte, self.row, self.column);
                self.column += 1;
            }
        }
    }

    fn write_char_unchecked(&mut self, char: ScreenChar, row: usize, col: usize) {
        if let Some(c) = self.buffer.get_mut(row, col) {
            c.write(char);
        }
    }

    fn write_byte_unchecked(&mut self, byte: u8, row: usize, col: usize) {
        if let Some(c) = self.buffer.get_mut(row, col) {
            c.update(|c| {
                c.char(byte).fg(VGAColor::White).bg(VGAColor::Black);
            });
        }
    }

    fn shift_up(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = *self.buffer.get(row, col).unwrap().read();
                self.write_char_unchecked(character, row - 1, col);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column = 0;
    }

    fn shift_down(&mut self) {
        for row in (0..BUFFER_HEIGHT - 1).rev() {
            for col in 0..BUFFER_WIDTH {
                let character = *self.buffer.get(row, col).unwrap().read();
                self.write_char_unchecked(character, row + 1, col);
            }
        }
        self.column = BUFFER_WIDTH - 1;
    }

    fn clear_row(&mut self, row: usize) {
        for col in 0..BUFFER_WIDTH {
            self.write_byte_unchecked(b' ', row, col);
        }
    }

    pub fn select(&mut self, row: usize, col: usize) -> Option<&mut Volatile<ScreenChar>> {
        self.buffer.get_mut(row, col)
    }

    pub fn select_prev(&mut self) -> Option<&mut Volatile<ScreenChar>> {
        let (row, col) = if self.column > 0 {
            (self.row, self.column - 1)
        } else if self.row > 0 {
            (self.row - 1, 0)
        } else {
            (self.row, self.column)
        };
        self.select(row, col)
    }

    pub fn select_next(&mut self) -> Option<&mut Volatile<ScreenChar>> {
        let (row, col) = if self.column < BUFFER_WIDTH - 1 {
            (self.row, self.column + 1)
        } else if self.row < BUFFER_HEIGHT - 1 {
            (self.row + 1, 0)
        } else {
            (self.row, self.column)
        };
        self.select(row, col)
    }

    pub fn select_row(&mut self, row: usize) -> Option<ScreenRow<'_>> {
        self.buffer
            .get_row_mut(row)
            .map(|row| ScreenRow { chars: row })
    }

    pub fn set_prev_pos(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        } else {
            self.shift_down();
        }
    }

    pub fn set_next_pos(&mut self) {
        if self.column < BUFFER_WIDTH - 1 {
            self.column += 1;
        } else {
            self.shift_up();
        }
    }

    pub fn current_pos(&self) -> (usize, usize) {
        (self.row, self.column)
    }
}

impl fmt::Write for VGAWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
        Ok(())
    }
}

lazy_static! {
    pub static ref WRITER: Mutex<VGAWriter> = Mutex::new(VGAWriter {
        column: 0,
        row: BUFFER_HEIGHT - 1,
        buffer: unsafe { &mut *VGABuffer::new() },
    });
}
