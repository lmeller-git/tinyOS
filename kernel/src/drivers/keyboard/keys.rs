use pc_keyboard::{EventDecoder, HandleControl, Keyboard, ScancodeSet1, layouts};

use super::KeyboardError;
use crate::sync::locks::Mutex;

pub static KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(Keyboard::new(
    ScancodeSet1::new(),
    layouts::Us104Key,
    HandleControl::MapLettersToUnicode,
));

pub fn parse_scancode(scancode: u8) -> Result<pc_keyboard::DecodedKey, KeyboardError> {
    let mut keyboard = KEYBOARD.lock();
    if let Some(res) = keyboard.add_byte(scancode)? {
        if let Some(res) = keyboard.process_keyevent(res) {
            return Ok(res);
        }
    }
    Err(KeyboardError::UnknownError(
        "no result from adding keyboard byte".into(),
    ))
}
