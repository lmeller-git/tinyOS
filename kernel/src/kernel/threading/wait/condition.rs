use alloc::boxed::Box;
use core::time::Duration;

use tinyos_abi::flags::{TaskStateChange, TaskWaitOptions};

use crate::{
    arch::x86::current_time,
    drivers::keyboard::KEYBOARD_BUFFER,
    kernel::threading::{
        task::{TaskRepr, TaskState, ThreadID},
        tls,
    },
};

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub enum WaitCondition {
    Time(Duration),
    Keyboard,
    Thread(ThreadID, TaskWaitOptions),
    Generic(u64, usize),
    None,
}

impl WaitCondition {
    //TODO add msg: T, where msg is the msg passed in WaitEvent
    pub fn is_given(&self) -> bool {
        match self {
            Self::Time(t) => *t <= current_time(),
            Self::Keyboard => !KEYBOARD_BUFFER.is_empty(),
            Self::Thread(id, config) => tls::task_data()
                .thread(id)
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
            Self::Generic(val, callback) => {
                let callback = unsafe { &*(*callback as *mut () as *mut dyn Fn(u64) -> bool) };
                callback(*val)
            }
            Self::None => true,
        }
    }
}
