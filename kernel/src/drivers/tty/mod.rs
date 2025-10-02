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
            KeyCode::Delete => {
                if buf.len() < 4 {
                    return -1;
                };
                buf[..4].copy_from_slice(&[ControlCode::ESC as u8, b'[', b'3', b'~']);
                4
            }
            KeyCode::ArrowUp => {
                if buf.len() < 3 {
                    return -1;
                };
                buf[..3].copy_from_slice(&[ControlCode::ESC as u8, b'[', b'A']);
                3
            }
            KeyCode::ArrowLeft => {
                if buf.len() < 3 {
                    return -1;
                };
                buf[..3].copy_from_slice(&[ControlCode::ESC as u8, b'[', b'D']);
                3
            }
            KeyCode::ArrowDown => {
                if buf.len() < 3 {
                    return -1;
                };
                buf[..3].copy_from_slice(&[ControlCode::ESC as u8, b'[', b'B']);
                3
            }
            KeyCode::ArrowRight => {
                if buf.len() < 3 {
                    return -1;
                };
                buf[..3].copy_from_slice(&[ControlCode::ESC as u8, b'[', b'C']);
                3
            }
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
