use alloc::{boxed::Box, string::String, sync::Arc};
use spin::{RwLockReadGuard, rwlock::RwLockWriteGuard};

use crate::{
    arch::{
        context::{
            KTaskInfo, TaskCtx, UsrTaskInfo, allocate_kstack, allocate_userkstack,
            allocate_userstack, init_kernel_task, init_usr_task,
        },
        current_page_tbl,
        mem::{Cr3Flags, PhysFrame, Size4KiB, VirtAddr},
    },
    kernel::{mem::paging::create_new_pagedir, threading::trampoline::TaskExitInfo},
    serial_println,
};
use core::{
    fmt::Debug,
    marker::PhantomData,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{ThreadingError, schedule::TaskPtr_};

pub trait TaskRepr: Debug {
    fn krsp(&mut self) -> &mut VirtAddr;
    fn kill(&mut self);
    fn kill_with_code(&mut self, code: usize);
    fn exit_info(&self) -> &TaskExitInfo;
    fn get_mut_exit_info(&mut self) -> &mut TaskExitInfo;
}

#[repr(C)]
#[derive(Debug)]
pub struct SimpleTask {
    pub krsp: VirtAddr,
    pub frame_flags: Cr3Flags,
    // pub ktop: VirtAddr,
    pub parent: Option<TaskID>,
    pub root_frame: PhysFrame<Size4KiB>,
    pub pid: TaskID,
    pub name: Option<String>,
    pub state: TaskState,
    pub exit_info: Pin<Box<TaskExitInfo>>,
    private_marker: PhantomData<u8>,
}

impl SimpleTask {
    fn new() -> Result<Self, ThreadingError> {
        // let stack_top = allocate_kstack()?;
        let (tbl, flags) = current_page_tbl();
        Ok(Self {
            krsp: VirtAddr::zero(),
            frame_flags: flags,
            // ktop: stack_top,
            parent: None,
            root_frame: tbl,
            pid: get_pid(),
            name: None,
            state: TaskState::Ready,
            private_marker: PhantomData,
            exit_info: Box::pin(TaskExitInfo::default()),
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

    fn kill_with_code(&mut self, code: usize) {
        self.state = TaskState::Zombie(ExitInfo {
            exit_code: code as u32,
            signal: None,
        })
    }

    fn exit_info(&self) -> &TaskExitInfo {
        self.exit_info.as_ref().get_ref()
    }

    fn get_mut_exit_info(&mut self) -> &mut TaskExitInfo {
        self.exit_info.as_mut().get_mut()
    }
}

pub struct Uninit;
pub struct Init;
pub struct Ready<I> {
    inner: I,
    exit: TaskExitInfo,
}

impl From<KTaskInfo> for Ready<KTaskInfo> {
    fn from(value: KTaskInfo) -> Self {
        Self {
            inner: value,
            exit: TaskExitInfo::default(),
        }
    }
}

impl From<UsrTaskInfo> for Ready<UsrTaskInfo> {
    fn from(value: UsrTaskInfo) -> Self {
        Self {
            inner: value,
            exit: TaskExitInfo::default(),
        }
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

    pub fn with_exit_info(mut self, exit_info: TaskExitInfo) -> TaskBuilder<SimpleTask, S> {
        *self.inner.get_mut_exit_info() = exit_info;
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

    pub fn from_fn(
        func: extern "C" fn() -> usize,
    ) -> Result<TaskBuilder<SimpleTask, Init>, ThreadingError> {
        Ok(TaskBuilder::<SimpleTask, Init> {
            inner: SimpleTask::new()?,
            entry: VirtAddr::new(func as usize as u64),
            _marker: Init,
        })
    }
}

impl TaskBuilder<SimpleTask, Init> {
    pub fn as_kernel(
        mut self,
    ) -> Result<TaskBuilder<SimpleTask, Ready<KTaskInfo>>, ThreadingError> {
        let stack_top = allocate_kstack()?;
        *self.inner.krsp() = stack_top;
        let info = KTaskInfo::new(self.entry, self.inner.krsp);
        Ok(TaskBuilder {
            inner: self.inner,
            entry: self.entry,
            _marker: info.into(),
        })
    }

    pub fn as_usr(mut self) -> Result<TaskBuilder<SimpleTask, Ready<UsrTaskInfo>>, ThreadingError> {
        let mut tbl = create_new_pagedir().map_err(|e| ThreadingError::PageDirNotBuilt)?;
        let usr_end = allocate_userstack(&mut tbl)?;
        let kstack = allocate_userkstack(&mut tbl)?;
        *self.inner.krsp() = kstack;
        let info = UsrTaskInfo::new(
            self.entry,
            self.inner.krsp,
            usr_end,
            tbl.root.start_address(),
        );
        Ok(TaskBuilder {
            inner: self.inner,
            entry: self.entry,
            _marker: info.into(),
        })
    }
}

impl<T: TaskRepr> TaskBuilder<T, Ready<UsrTaskInfo>> {
    pub fn build(mut self) -> T {
        serial_println!("data: {:#?}", self._marker.inner);
        serial_println!("task: {:#?}", self.inner);

        let next_top = unsafe { init_usr_task(&self._marker.inner, self.inner.exit_info()) };

        serial_println!("krsp after pushes: {:#x}", next_top);
        *self.inner.krsp() = next_top;
        self.inner
    }
}

impl<T: TaskRepr> TaskBuilder<T, Ready<KTaskInfo>> {
    pub fn build(mut self) -> T {
        #[cfg(not(feature = "test_run"))]
        serial_println!("krsp: {:#x}", self.inner.krsp());
        #[cfg(not(feature = "test_run"))]
        serial_println!("task info: {:#?}", self._marker.inner);

        let next_top = unsafe { init_kernel_task(&self._marker.inner, self.inner.exit_info()) };

        #[cfg(not(feature = "test_run"))]
        serial_println!("krsp after pushes: {:#x}", next_top);
        *self.inner.krsp() = next_top;
        self.inner
    }
}

impl TaskBuilder<Task, Uninit> {
    pub unsafe fn from_addr(addr: VirtAddr) -> Result<TaskBuilder<Task, Init>, ThreadingError> {
        todo!()
    }
}

#[derive(Debug)]
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

    fn kill_with_code(&mut self, code: usize) {
        self.state = TaskState::Zombie(ExitInfo {
            exit_code: code as u32,
            signal: None,
        })
    }
    fn exit_info(&self) -> &TaskExitInfo {
        todo!()
    }
    fn get_mut_exit_info(&mut self) -> &mut TaskExitInfo {
        todo!()
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

#[repr(C)]
#[derive(Debug, Default)]
pub struct TaskPtr<T: TaskRepr> {
    inner: TaskPtr_<T>,
}

impl<T: TaskRepr> TaskPtr<T> {
    pub fn new(ptr: TaskPtr_<T>) -> Self {
        Self { inner: ptr }
    }

    pub fn try_into_inner(self) -> Option<T> {
        Arc::try_unwrap(self.inner)
            .ok()
            .map(|inner| inner.into_inner())
    }

    pub fn into_raw(self) -> TaskPtr_<T> {
        self.inner
    }

    pub fn raw(&self) -> &TaskPtr_<T> {
        &self.inner
    }

    pub fn with_inner<F, R>(&self, func: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let guard = self.inner.read();
        func(&*guard)
    }

    pub fn with_inner_mut<F, R>(&self, func: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let mut guard = self.inner.write();
        func(&mut *guard)
    }

    pub fn read_inner(&self) -> RwLockReadGuard<T> {
        self.inner.read()
    }

    pub fn write_inner(&self) -> RwLockWriteGuard<T> {
        self.inner.write()
    }
}

impl<T: TaskRepr> From<T> for TaskPtr<T> {
    fn from(value: T) -> Self {
        Self {
            inner: TaskPtr_::new(value.into()),
        }
    }
}

impl<T: TaskRepr> Clone for TaskPtr<T> {
    fn clone(&self) -> Self {
        Self::new(self.inner.clone())
    }
}
