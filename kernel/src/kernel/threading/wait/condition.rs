use core::time::Duration;

use tinyos_abi::flags::{TaskStateChange, TaskWaitOptions};

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
    Thread(TaskID, TaskWaitOptions),
    None,
}

impl WaitCondition {
    //TODO add msg: T, where msg is the msg passed in WaitEvent
    pub fn is_given(&self) -> bool {
        match self {
            Self::Time(t) => *t <= current_time(),
            Self::Keyboard => !KEYBOARD_BUFFER.is_empty(),
            Self::Thread(id, config) => tls::task_data()
                .get(id)
                .and_then(|t| {
                    Some(
                        (config.contains(TaskWaitOptions::W_EXIT)
                            && t.state() == TaskState::Zombie)
                            || (config.contains(TaskWaitOptions::W_WAKEUP)
                                && (t.state() == TaskState::Ready
                                    || t.state() == TaskState::Running))
                            || (config.contains(TaskWaitOptions::W_BLOCK)
                                && t.state() == TaskState::Blocking),
                    )
                })
                .is_none_or(|r| r),
            Self::None => true,
        }
    }
}
