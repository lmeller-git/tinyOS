use core::time::Duration;

use crate::{
    arch::x86::current_time,
    drivers::keyboard::KEYBOARD_BUFFER,
    kernel::threading::{
        task::{TaskID, TaskRepr, TaskState},
        tls,
    },
};

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub enum WaitCondition {
    Time(Duration),
    Keyboard,
    Thread(TaskID),
    None,
}

impl WaitCondition {
    pub fn is_given(&self) -> bool {
        match self {
            Self::Time(t) => *t <= current_time(),
            Self::Keyboard => !KEYBOARD_BUFFER.is_empty(),
            Self::Thread(id) => tls::task_data()
                .get(id)
                .and_then(|t| Some(t.state() == TaskState::Zombie))
                .is_none_or(|r| r),
            Self::None => true,
        }
    }
}
