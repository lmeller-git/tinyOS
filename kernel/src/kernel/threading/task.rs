use alloc::string::String;

use crate::{
    arch::{
        context::{
            KTaskInfo, TaskCtx, UsrTaskInfo, allocate_kstack, allocate_userkstack,
            allocate_userstack, init_kernel_task,
        },
        current_page_tbl,
        interrupt::gdt::get_kernel_selectors,
        mem::{Cr3Flags, PhysFrame, Size4KiB, VirtAddr},
    },
    bootinfo,
    kernel::mem::paging::create_new_pagedir,
    serial_println,
};
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

use super::ThreadingError;

pub trait TaskRepr {
    fn krsp(&mut self) -> &mut VirtAddr;
    fn kill(&mut self);
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct SimpleTask {
    pub krsp: VirtAddr,
    pub frame_flags: Cr3Flags,
    // pub ktop: VirtAddr,
    pub parent: Option<TaskID>,
    pub root_frame: PhysFrame<Size4KiB>,
    pub pid: TaskID,
    pub name: Option<String>,
    pub state: TaskState,
}

impl SimpleTask {
    fn new() -> Result<Self, ThreadingError> {
        let stack_top = allocate_kstack()?;
        let (tbl, flags) = current_page_tbl();
        Ok(Self {
            krsp: stack_top,
            frame_flags: flags,
            // ktop: stack_top,
            parent: None,
            root_frame: tbl,
            pid: get_pid(),
            name: None,
            state: TaskState::Ready,
        })
    }
}

impl TaskRepr for SimpleTask {
    fn krsp(&mut self) -> &mut VirtAddr {
        &mut self.krsp
    }

    fn kill(&mut self) {
        self.state = TaskState::Zombie(ExitInfo {
            exit_code: 1,
            signal: None,
        })
    }
}

pub struct Uninit;
pub struct Init;
pub struct Ready<I> {
    inner: I,
}

impl From<KTaskInfo> for Ready<KTaskInfo> {
    fn from(value: KTaskInfo) -> Self {
        Self { inner: value }
    }
}

pub struct TaskBuilder<T: TaskRepr, S> {
    inner: T,
    entry: VirtAddr,
    _marker: S,
}

impl<T, S> TaskBuilder<T, S> where T: TaskRepr {}

impl<S> TaskBuilder<SimpleTask, S> {
    pub fn with_name(mut self, name: String) -> TaskBuilder<SimpleTask, S> {
        self.inner.name.replace(name);
        self
    }
}

impl TaskBuilder<SimpleTask, Uninit> {
    pub unsafe fn from_addr(
        addr: VirtAddr,
    ) -> Result<TaskBuilder<SimpleTask, Init>, ThreadingError> {
        Ok(TaskBuilder::<SimpleTask, Init> {
            inner: SimpleTask::new()?,
            entry: addr,
            _marker: Init,
        })
    }

    pub fn from_fn(func: extern "C" fn()) -> Result<TaskBuilder<SimpleTask, Init>, ThreadingError> {
        Ok(TaskBuilder::<SimpleTask, Init> {
            inner: SimpleTask::new()?,
            entry: VirtAddr::new(func as usize as u64),
            _marker: Init,
        })
    }
}

impl TaskBuilder<SimpleTask, Init> {
    pub fn as_kernel(mut self) -> TaskBuilder<SimpleTask, Ready<KTaskInfo>> {
        let info = KTaskInfo::new(self.entry, self.inner.krsp);
        TaskBuilder {
            inner: self.inner,
            entry: self.entry,
            _marker: info.into(),
        }
    }

    pub fn as_usr(mut self) -> TaskBuilder<SimpleTask, Ready<UsrTaskInfo>> {
        todo!()
    }
}

impl<T: TaskRepr> TaskBuilder<T, Ready<UsrTaskInfo>> {
    pub fn build(mut self) -> T {
        todo!()
    }
}

impl<T: TaskRepr> TaskBuilder<T, Ready<KTaskInfo>> {
    pub fn build(mut self) -> T {
        serial_println!("krsp: {:x}", self.inner.krsp());
        serial_println!("task info: {:#?}", self._marker.inner);
        let (cs, ss) = get_kernel_selectors();
        let next_top = unsafe { init_kernel_task(&self._marker.inner) };

        serial_println!("krsp after pushes: {:x}", next_top);
        *self.inner.krsp() = next_top;
        self.inner
    }
}

impl<S> TaskBuilder<Task, S> {
    pub unsafe fn from_addr(addr: VirtAddr) -> Result<Self, ThreadingError> {
        todo!()
    }
}

pub struct Task {
    // pub(super) kstack_rsp: Option<VirtAddr>,
    pub(super) ctx: TaskCtx,
    pub(super) state: TaskState,
    pub(super) parent: Option<TaskID>,
    pub(super) root_frame: PhysFrame<Size4KiB>,
    pub(super) frame_flags: Cr3Flags,
    pub(super) kstack_top: Option<VirtAddr>,
    pid: TaskID,
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

    pub fn pid(&self) -> &TaskID {
        &self.pid
    }
}

impl TaskRepr for Task {
    fn krsp(&mut self) -> &mut VirtAddr {
        todo!()
    }

    fn kill(&mut self) {
        self.state = TaskState::Zombie(ExitInfo {
            exit_code: 1,
            signal: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitInfo {
    pub exit_code: u32,
    pub signal: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct TaskID {
    inner: u64,
}

pub fn get_pid() -> TaskID {
    static CURRENT_PID: AtomicU64 = AtomicU64::new(0);
    let current = CURRENT_PID.fetch_add(1, Ordering::Relaxed);
    TaskID { inner: current }
}
