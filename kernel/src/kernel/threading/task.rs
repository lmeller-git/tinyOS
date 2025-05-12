use crate::{
    arch::{
        context::{TaskCtx, allocate_kstack, allocate_userkstack, allocate_userstack},
        current_page_tbl,
        mem::{Cr3Flags, PhysFrame, Size4KiB, VirtAddr},
    },
    kernel::mem::paging::create_new_pagedir,
};
use core::sync::atomic::{AtomicU64, Ordering};

use super::ThreadingError;

pub struct Task {
    pub(super) pid: TaskID,
    pub(super) ctx: TaskCtx,
    pub(super) state: TaskState,
    pub(super) parent: Option<TaskID>,
    pub(super) root_frame: PhysFrame<Size4KiB>,
    pub(super) frame_flags: Cr3Flags,
    pub(super) kstack_top: Option<VirtAddr>,
}

impl Task {
    pub fn new_kernel(entry: extern "C" fn()) -> Result<Self, ThreadingError> {
        let stack_top = allocate_kstack()?;
        let (tbl, flags) = current_page_tbl();
        Ok(Self {
            pid: get_pid(),
            ctx: TaskCtx::new_kernel(entry as usize, stack_top),
            state: TaskState::new(),
            parent: None,
            root_frame: tbl,
            frame_flags: flags,
            kstack_top: None,
        })
    }

    pub fn new_user(entry: extern "C" fn()) -> Result<Self, ThreadingError> {
        let (tbl, flags) = current_page_tbl();
        let mut new_tbl = create_new_pagedir().map_err(|_| ThreadingError::PageDirNotBuilt)?;
        let kstack_top = allocate_userkstack(&mut new_tbl)?;
        let stack_top = allocate_userstack(&mut new_tbl)?;

        Ok(Self {
            pid: get_pid(),
            ctx: TaskCtx::new_user(entry as usize, stack_top),
            state: TaskState::new(),
            parent: None,
            root_frame: new_tbl.root,
            frame_flags: flags, // ?
            kstack_top: Some(kstack_top),
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
