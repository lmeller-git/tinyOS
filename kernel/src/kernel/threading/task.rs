use super::{ProcessEntry, ThreadingError, schedule::TaskPtr_};
use crate::{
    add_device,
    arch::{
        self,
        context::{
            KTaskInfo, TaskCtx, UsrTaskInfo, allocate_kstack, allocate_userkstack,
            allocate_userstack, copy_ustack_mappings_into, init_kernel_task, init_usr_task,
            unmap_ustack_mappings,
        },
        current_page_tbl, interrupt,
        mem::{Cr3Flags, PhysFrame, Size4KiB, VirtAddr},
    },
    kernel::{
        devices::{Attacheable, CompositeAttacheable, FdEntry, FdTag, TaskDevices},
        elf::apply,
        mem::paging::{PAGETABLE, TaskPageTable, create_new_pagedir},
        threading::trampoline::TaskExitInfo,
    },
    locks::reentrant::{RwLockReadGuard, RwLockWriteGuard},
    serial_println,
};
use alloc::{boxed::Box, format, string::String, sync::Arc, vec::Vec};
use core::{
    fmt::{Debug, LowerHex},
    marker::PhantomData,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
};
use elf::{ElfBytes, endian::AnyEndian};
use hashbrown::HashMap;

pub trait TaskRepr: Debug {
    fn krsp(&mut self) -> &mut VirtAddr;
    fn get_krsp(&self) -> &VirtAddr;
    fn kill(&mut self);
    fn kill_with_code(&mut self, code: usize);
    fn exit_info(&self) -> &TaskExitInfo;
    fn get_mut_exit_info(&mut self) -> &mut TaskExitInfo;
    fn block(&mut self) {}
    fn wake(&mut self) {}
    fn get_devices(&self) -> &TaskDevices;
    fn get_devices_mut(&mut self) -> &mut TaskDevices;
    fn privilege_level(&self) -> PrivilegeLevel;
    fn mut_pagdir(&mut self) -> &mut TaskPageTable<'static> {
        todo!()
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrivilegeLevel {
    Kernel,
    User,
    #[default]
    Unset,
}

#[repr(C)]
#[derive(Debug)]
pub struct SimpleTask {
    pub krsp: VirtAddr,
    pub frame_flags: Cr3Flags,
    pub parent: Option<TaskID>,
    pub root_frame: PhysFrame<Size4KiB>,
    pub pid: TaskID,
    pub name: Option<String>,
    pub state: TaskState,
    pub exit_info: Pin<Box<TaskExitInfo>>,
    pub devices: TaskDevices,
    pub privilege: PrivilegeLevel,
    pub heap_size: usize,
    pub pagedir: Option<TaskPageTable<'static>>,
    private_marker: PhantomData<u8>,
}

impl SimpleTask {
    fn new() -> Result<Self, ThreadingError> {
        let (tbl, flags) = current_page_tbl();
        Ok(Self {
            krsp: VirtAddr::zero(),
            frame_flags: flags,
            parent: None,
            root_frame: tbl,
            pid: get_pid(),
            name: None,
            state: TaskState::Ready,
            private_marker: PhantomData,
            devices: TaskDevices::new(),
            privilege: PrivilegeLevel::default(),
            heap_size: 0,
            pagedir: None,
            exit_info: Box::pin(TaskExitInfo::default()),
        })
    }
}

impl TaskRepr for SimpleTask {
    fn krsp(&mut self) -> &mut VirtAddr {
        &mut self.krsp
    }

    fn get_krsp(&self) -> &VirtAddr {
        &self.krsp
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

    fn block(&mut self) {
        self.state = TaskState::Blocking;
    }

    fn wake(&mut self) {
        if self.state == TaskState::Blocking {
            self.state = TaskState::Ready;
        }
    }

    fn get_devices(&self) -> &TaskDevices {
        &self.devices
    }

    fn get_devices_mut(&mut self) -> &mut TaskDevices {
        &mut self.devices
    }

    fn privilege_level(&self) -> PrivilegeLevel {
        self.privilege
    }

    fn mut_pagdir(&mut self) -> &mut TaskPageTable<'static> {
        self.pagedir.as_mut().unwrap()
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Arg(usize);

impl Arg {
    pub fn from_usize(v: usize) -> Self {
        Self(v)
    }

    pub fn from_ptr<T>(ptr: *mut T) -> Self {
        Self(ptr as usize)
    }

    pub fn from_val<T>(v: T) -> Self {
        let boxed = Box::new(v);
        Self::from_ptr(Box::into_raw(boxed))
    }

    pub fn from_fn<F>(func: F) -> Self
    where
        F: FnOnce() + 'static + Send + Sync,
    {
        let boxed: Box<dyn FnOnce() + Send + Sync + 'static> = Box::new(func);
        let ptr = Box::new(boxed);
        Self::from_ptr(Box::into_raw(ptr))
    }

    pub unsafe fn as_val<T>(&self) -> T {
        let boxed = Box::from_raw(self.0 as *mut T);
        *boxed
    }

    pub unsafe fn as_closure(&self) -> Box<dyn FnOnce() + 'static + Send + Sync> {
        *Box::from_raw(self.0 as *mut Box<dyn FnOnce() + 'static + Send + Sync>)
    }
}

impl Default for Arg {
    fn default() -> Self {
        Self(42)
    }
}

impl LowerHex for Arg {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#x}", self.0)?;
        Ok(())
    }
}

#[repr(transparent)]
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Args([Arg; 6]);

impl Args {
    pub fn new(s: [Arg; 6]) -> Self {
        Self(s)
    }

    pub fn get_mut(&mut self, idx: usize) -> &mut Arg {
        self.0.get_mut(idx).expect("cannot index over max_args")
    }

    pub fn get(&self, idx: usize) -> &Arg {
        self.0.get(idx).expect("cannot index over max_args")
    }
}

#[macro_export]
macro_rules! args {
    ($($arg:expr),* $(,)?) => {{
        const MAX_ARGS: usize = 6;
        let mut arr = [crate::kernel::threading::task::Arg::default(); MAX_ARGS];
        let mut idx = 0;
        $(
            if idx < MAX_ARGS {
                arr[idx] = crate::kernel::threading::task::Arg::from_val($arg);
                idx += 1;
            }
        )*
        crate::kernel::threading::task::Args::new(arr)
    }};
}

pub struct Uninit;
pub struct Init<'data> {
    elf_data: Option<&'data [u8]>,
}

impl<'data> Init<'data> {
    fn new(bytes: &'data [u8]) -> Self {
        Self {
            elf_data: Some(bytes),
        }
    }
}

impl Default for Init<'_> {
    fn default() -> Self {
        Self { elf_data: None }
    }
}

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

impl<'a> From<ExtendedUsrTaskInfo<'a>> for Ready<ExtendedUsrTaskInfo<'a>> {
    fn from(value: ExtendedUsrTaskInfo<'a>) -> Self {
        Self {
            inner: value,
            exit: TaskExitInfo::default(),
        }
    }
}
#[repr(C)]
#[derive(Default, Debug)]
pub struct TaskData {
    args: Args,
}

pub struct TaskBuilder<T: TaskRepr, S> {
    inner: T,
    entry: VirtAddr,
    data: TaskData,
    _marker: S,
}

pub struct ExtendedUsrTaskInfo<'a> {
    info: UsrTaskInfo,
    _phatom: PhantomData<&'a u8>,
}

impl<T, S> TaskBuilder<T, S>
where
    T: TaskRepr,
{
    pub fn with_args(mut self, args: Args) -> Self {
        self.data.args = args;
        self
    }
}

impl<S> TaskBuilder<SimpleTask, S> {
    pub fn with_name(mut self, name: String) -> TaskBuilder<SimpleTask, S> {
        self.inner.name.replace(name);
        self
    }

    pub fn with_exit_info(mut self, exit_info: TaskExitInfo) -> TaskBuilder<SimpleTask, S> {
        *self.inner.get_mut_exit_info() = exit_info;
        self
    }

    pub fn with_device<T>(mut self, device: FdEntry<T>) -> TaskBuilder<SimpleTask, S>
    where
        T: FdTag,
        FdEntry<T>: Attacheable,
    {
        self.inner.get_devices_mut().attach(device);
        self
    }

    pub fn with_default_devices(mut self) -> TaskBuilder<SimpleTask, S> {
        self.inner.devices = self.inner.devices.add_default();
        self
    }
}

impl TaskBuilder<SimpleTask, Uninit> {
    pub unsafe fn from_addr(
        addr: VirtAddr,
    ) -> Result<TaskBuilder<SimpleTask, Init<'static>>, ThreadingError> {
        Ok(TaskBuilder::<SimpleTask, Init> {
            inner: SimpleTask::new()?,
            entry: addr,
            data: TaskData::default(),
            _marker: Init::default(),
        })
    }

    pub fn from_fn(
        func: ProcessEntry,
    ) -> Result<TaskBuilder<SimpleTask, Init<'static>>, ThreadingError> {
        Ok(TaskBuilder::<SimpleTask, Init> {
            inner: SimpleTask::new()?,
            entry: VirtAddr::new(func as usize as u64),
            data: TaskData::default(),
            _marker: Init::default(),
        })
    }

    pub fn from_bytes<'data>(
        bytes: &'data [u8],
    ) -> Result<TaskBuilder<SimpleTask, Init<'data>>, ThreadingError> {
        Ok(TaskBuilder::<SimpleTask, Init> {
            inner: SimpleTask::new()?,
            entry: VirtAddr::zero(),
            data: TaskData::default(),
            _marker: Init::new(bytes),
        })
    }
}

impl TaskBuilder<SimpleTask, Init<'_>> {
    pub fn as_kernel(
        mut self,
    ) -> Result<TaskBuilder<SimpleTask, Ready<KTaskInfo>>, ThreadingError> {
        let stack_top = allocate_kstack()?;
        *self.inner.krsp() = stack_top;
        self.inner.privilege = PrivilegeLevel::Kernel;
        let info = KTaskInfo::new(self.entry, self.inner.krsp);
        Ok(TaskBuilder {
            inner: self.inner,
            entry: self.entry,
            data: self.data,
            _marker: info.into(),
        })
    }

    pub fn as_usr<'a>(
        mut self,
    ) -> Result<TaskBuilder<SimpleTask, Ready<ExtendedUsrTaskInfo<'a>>>, ThreadingError> {
        let kstack = allocate_kstack()?;
        let mut tbl =
            create_new_pagedir::<'a, '_>().map_err(|e| ThreadingError::PageDirNotBuilt)?;
        let usr_end = allocate_userstack(&mut tbl)?;
        // let kstack = allocate_userkstack(&mut tbl)?;
        *self.inner.krsp() = kstack;
        self.inner.privilege = PrivilegeLevel::User;

        if let Some(data) = self._marker.elf_data {
            let bytes = elf::ElfBytes::minimal_parse(data)
                .map_err(|e| ThreadingError::Unknown(format!("{:#?}", e)))?;
            self.entry = VirtAddr::new(bytes.ehdr.e_entry);
            apply(&bytes, data, &mut tbl)
                .map_err(|e| ThreadingError::Unknown(format!("{:#?}", e)))?;
        }

        let info = UsrTaskInfo::new(
            self.entry,
            self.inner.krsp,
            usr_end,
            tbl.root.start_address(),
        );

        let _marker = ExtendedUsrTaskInfo {
            info: info,
            _phatom: PhantomData,
        }
        .into();

        self.inner.pagedir = Some(tbl);

        Ok(TaskBuilder {
            inner: self.inner,
            entry: self.entry,
            data: self.data,
            _marker,
        })
    }
}

impl<T: TaskRepr> TaskBuilder<T, Ready<ExtendedUsrTaskInfo<'_>>> {
    pub fn build(mut self) -> T {
        // serial_println!("data: {:#?}", self._marker.inner);
        // serial_println!("task: {:#?}", self.inner);

        unsafe {
            interrupt::disable();
        }

        copy_ustack_mappings_into(self.inner.mut_pagdir(), &mut *PAGETABLE.lock());

        let next_top =
            unsafe { init_usr_task(&self._marker.inner.info, self.inner.exit_info(), &self.data) };

        unmap_ustack_mappings(&mut *PAGETABLE.lock());
        unsafe {
            interrupt::enable();
        }

        // serial_println!("krsp after pushes: {:#x}", next_top);
        *self.inner.krsp() = next_top;
        self.inner
    }
}

impl<T: TaskRepr> TaskBuilder<T, Ready<KTaskInfo>> {
    pub fn build(mut self) -> T {
        // serial_println!("spawnning task");
        // #[cfg(not(feature = "test_run"))]
        // serial_println!("krsp: {:#x}", self.inner.krsp());
        // #[cfg(not(feature = "test_run"))]
        // serial_println!("task info: {:#?}", self._marker.inner);

        // serial_println!("task data: {:#?}", self.data.args);
        let next_top =
            unsafe { init_kernel_task(&self._marker.inner, self.inner.exit_info(), &self.data) };

        // #[cfg(not(feature = "test_run"))]
        // serial_println!("krsp after pushes: {:#x}", next_top);
        *self.inner.krsp() = next_top;
        self.inner
    }
}

impl TaskBuilder<Task, Uninit> {
    pub unsafe fn from_addr(
        addr: VirtAddr,
    ) -> Result<TaskBuilder<Task, Init<'static>>, ThreadingError> {
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

    fn get_krsp(&self) -> &VirtAddr {
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
    fn block(&mut self) {
        todo!()
    }

    fn wake(&mut self) {
        todo!()
    }

    fn get_devices(&self) -> &TaskDevices {
        todo!()
    }
    fn get_devices_mut(&mut self) -> &mut TaskDevices {
        todo!()
    }
    fn privilege_level(&self) -> PrivilegeLevel {
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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TaskID {
    inner: u64,
}

impl TaskID {
    pub fn get_inner(&self) -> u64 {
        self.inner
    }
}

impl From<u64> for TaskID {
    fn from(value: u64) -> Self {
        Self { inner: value }
    }
}

pub fn get_pid() -> TaskID {
    // PIDs start at 1 since locks use 0 as default value for "held by thread x"
    static CURRENT_PID: AtomicU64 = AtomicU64::new(1);
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

#[cfg(feature = "test_run")]
mod tests {
    use super::*;
    use crate::kernel::threading::{ProcessReturn, spawn_fn};
    use os_macros::{kernel_test, with_default_args};

    #[kernel_test]
    fn zero_args() {
        let a0 = args!();
        assert_eq!(
            a0,
            Args::new([
                Arg::default(),
                Arg::default(),
                Arg::default(),
                Arg::default(),
                Arg::default(),
                Arg::default()
            ])
        );
    }

    #[kernel_test]
    fn closure_arg() {
        let handle = Arc::new(AtomicU64::new(0));
        let handle_clone = handle.clone();
        let arg = Arg::from_fn(move || {
            handle_clone.store(42, Ordering::Relaxed);
        });

        unsafe { (arg.as_closure())() };

        assert_eq!(handle.load(Ordering::Relaxed), 42);
    }

    #[kernel_test]
    fn any_args() {
        #[derive(Debug, Eq, PartialEq)]
        struct Foo {
            a: usize,
        };
        let args = args!(1, "hello", Foo { a: 1 }, Box::new(42));
        unsafe {
            assert_eq!(args.0[0].as_val::<usize>(), 1);
            assert_eq!(args.0[1].as_val::<&str>(), "hello");
            assert_eq!(args.0[2].as_val::<Foo>(), Foo { a: 1 });
            assert_eq!(args.0[3].as_val::<Box<usize>>(), Box::new(42));
            assert_eq!(args.0[4].0, Arg::default().0);
            assert_eq!(args.0[5].0, Arg::default().0);
        }
    }

    #[with_default_args]
    extern "C" fn foo() -> ProcessReturn {
        _arg0.0 + _arg1.0 + _arg2.0 + _arg3.0 + _arg4.0 + _arg5.0
    }

    #[with_default_args]
    extern "C" fn bar(v1: Arg) -> ProcessReturn {
        let v1 = unsafe { v1.as_val::<&str>() };
        assert_eq!(v1, "hello");
        assert_eq!(unsafe { _arg1.as_val::<i64>() }, 4242);
        assert_eq!(unsafe { _arg2.as_val::<Box<u8>>() }, Box::new(42));
        ProcessReturn::default()
    }

    #[kernel_test]
    fn with_args() {
        let handle = spawn_fn(foo, args!()).unwrap();
        let handle2 = spawn_fn(bar, args!("hello", 4242, Box::new(42))).unwrap();
        assert_eq!(handle.wait(), Ok(Arg::default().0 * 6));
        assert_eq!(handle2.wait(), Ok(ProcessReturn::default()));
    }
}
