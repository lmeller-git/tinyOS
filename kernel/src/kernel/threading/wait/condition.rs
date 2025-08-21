use core::time::Duration;

use crate::{arch::x86::current_time, drivers::keyboard::KEYBOARD_BUFFER};

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub enum WaitCondition {
    Time(Duration),
    Keyboard,
    None,
}

impl WaitCondition {
    pub fn is_given(&self) -> bool {
        match self {
            Self::Time(t) => *t <= current_time(),
            Self::Keyboard => !KEYBOARD_BUFFER.is_empty(),
            Self::None => true,
        }
    }
}
