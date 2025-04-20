use pc_keyboard::{HandleControl, Keyboard, ScancodeSet1, layouts};

use super::KeyboardError;

pub fn parse_scancode(scancode: u8) -> Result<pc_keyboard::DecodedKey, KeyboardError> {
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );
    if let Some(res) = keyboard.add_byte(scancode)? {
        if let Some(res) = keyboard.process_keyevent(res) {
            return Ok(res);
        }
    }
    Err(KeyboardError::UnknownError(
        "no result from adding keyboard byte".into(),
    ))
}
