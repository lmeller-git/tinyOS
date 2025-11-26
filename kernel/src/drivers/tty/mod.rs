use pc_keyboard::{DecodedKey, KeyCode};

use crate::{
    kernel::{
        devices::tty::{
            TTYSink,
            sink::{FBBACKEND, SERIALBACKEND},
        },
        threading,
    },
    serial_println,
};

//TODO add wake up logic
pub fn start_tty_backend() {
    _ = threading::spawn(move || {
        loop {
            SERIALBACKEND.get().unwrap().flush();
            FBBACKEND.get().unwrap().flush();
            threading::yield_now();
        }
    })
    .unwrap();
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCode {
    ESC = 0x1B,
    BEL = 0x07,
    LF = 0x0A,
    FF = 0x0C,
    CR = 0x0D,
    HT = 0x09,
    BS = 0x08,
}

fn write(buf: &mut [u8], bytes: &[u8]) -> isize {
    if buf.len() < bytes.len() {
        return -1;
    }
    buf[..bytes.len()].copy_from_slice(bytes);

    bytes.len() as isize
}

pub fn map_key(key: DecodedKey, buf: &mut [u8]) -> isize {
    match key {
        DecodedKey::RawKey(raw) => match raw {
            KeyCode::Escape => {
                if buf.len() < 1 {
                    return -1;
                };
                *buf.first_mut().unwrap() = ControlCode::ESC as u8;
                1
            }
            KeyCode::Backspace => {
                if buf.len() < 1 {
                    return -1;
                };
                *buf.first_mut().unwrap() = ControlCode::BS as u8;
                1
            }
            KeyCode::Delete => write(buf, &[ControlCode::ESC as u8, b'[', b'3', b'~']),
            KeyCode::ArrowUp => write(buf, &[ControlCode::ESC as u8, b'[', b'A']),
            KeyCode::ArrowLeft => write(buf, &[ControlCode::ESC as u8, b'[', b'D']),
            KeyCode::ArrowDown => write(buf, &[ControlCode::ESC as u8, b'[', b'B']),
            KeyCode::ArrowRight => write(buf, &[ControlCode::ESC as u8, b'[', b'C']),
            KeyCode::Home => write(buf, b"\x1B[H"),
            KeyCode::End => write(buf, b"\x1B[F"),
            KeyCode::PageUp => write(buf, b"\x1B[5~"),
            KeyCode::PageDown => write(buf, b"\x1B[6~"),
            KeyCode::Insert => write(buf, b"\x1B[2~"),
            KeyCode::F1 => write(buf, b"\x1BOP"),
            KeyCode::F2 => write(buf, b"\x1BOQ"),
            KeyCode::F3 => write(buf, b"\x1BOR"),
            KeyCode::F4 => write(buf, b"\x1BOS"),
            KeyCode::F5 => write(buf, b"\x1B[15~"),
            KeyCode::F6 => write(buf, b"\x1B[17~"),
            KeyCode::F7 => write(buf, b"\x1B[18~"),
            KeyCode::F8 => write(buf, b"\x1B[19~"),
            KeyCode::F9 => write(buf, b"\x1B[20~"),
            KeyCode::F10 => write(buf, b"\x1B[21~"),
            KeyCode::F11 => write(buf, b"\x1B[23~"),
            KeyCode::F12 => write(buf, b"\x1B[24~"),
            k => {
                serial_println!("not handled: {:#?}", k);
                0
            }
        },
        DecodedKey::Unicode(c) => {
            let length = c.len_utf8();
            c.encode_utf8(buf);
            length as isize
        }
    }
}
