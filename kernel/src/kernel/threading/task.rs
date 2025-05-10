use core::sync::atomic::{AtomicU64, Ordering};

use super::context::TaskCtx;

pub struct Task {
    pid: TaskID,
    ctx: TaskCtx,
    state: TaskState,
}

impl Task {
    fn new() -> Self {
        Self {
            pid: get_pid(),
            ctx: TaskCtx::new(),
            state: TaskState::new(),
        }
    }
}

pub enum TaskState {}

impl TaskState {
    pub fn new() -> Self {
        Self
    }
}

pub struct TaskID {
    inner: u64,
}

pub fn get_pid() -> TaskID {
    static CURRENT_PID: AtomicU64 = AtomicU64::new(0);
    let current = CURRENT_PID.fetch_add(1, Ordering::Relaxed);
    TaskID { inner: current }
}
