use alloc::string::String;

use thiserror::Error;

mod keys;
mod queue;
pub use keys::parse_scancode;
pub use queue::{KEYBOARD_BUFFER, get_current_next, get_next, put_scancode};

#[derive(Error, Debug)]
pub enum KeyboardError {
    #[error("queue is full")]
    FullQueue,
    #[error("empty queue")]
    EmptyQueue,
    #[error("failed with: {0:?}")]
    UnknownError(String),
    #[error("Bad start bit")]
    BadStartBit,
    #[error("Bad stop bit")]
    BadStopBit,
    #[error("unknown key code")]
    UnknownKeyCode,
}

impl From<pc_keyboard::Error> for KeyboardError {
    fn from(value: pc_keyboard::Error) -> Self {
        match value {
            pc_keyboard::Error::BadStartBit => Self::BadStartBit,
            pc_keyboard::Error::BadStopBit => Self::BadStopBit,
            pc_keyboard::Error::UnknownKeyCode => Self::UnknownKeyCode,
            _ => Self::UnknownError("pc_keyboard erred somehow".into()),
        }
    }
}
