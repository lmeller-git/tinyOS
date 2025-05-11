use crate::arch::{
    context::{TaskCtx, allocate_kstack},
    current_page_tbl,
    mem::{Cr3Flags, PhysFrame, Size4KiB, VirtAddr},
};
use core::sync::atomic::{AtomicU64, Ordering};

use super::ThreadingError;

pub struct Task {
    pid: TaskID,
    ctx: TaskCtx,
    state: TaskState,
    parent: Option<TaskID>,
    root_frame: PhysFrame<Size4KiB>,
    frame_flags: Cr3Flags,
}

impl Task {
    fn new_kernel(entry: extern "C" fn()) -> Result<Self, ThreadingError> {
        let stack_top = allocate_kstack()?;
        let (tbl, flags) = current_page_tbl();
        Ok(Self {
            pid: get_pid(),
            ctx: TaskCtx::new(entry as usize, 0, stack_top),
            state: TaskState::new(),
            parent: None,
            root_frame: tbl,
            frame_flags: flags,
        })
    }
}

pub enum TaskState {
    Running,
    Ready,
    Blocking,
    Sleeping,
    Zombie(ExitInfo),
}

impl TaskState {
    pub fn new() -> Self {
        Self::Ready
    }
}

pub struct ExitInfo {
    pub exit_code: u32,
    pub signal: Option<u8>,
}

pub struct TaskID {
    inner: u64,
}

pub fn get_pid() -> TaskID {
    static CURRENT_PID: AtomicU64 = AtomicU64::new(0);
    let current = CURRENT_PID.fetch_add(1, Ordering::Relaxed);
    TaskID { inner: current }
}
