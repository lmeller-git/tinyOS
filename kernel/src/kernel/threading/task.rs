use alloc::{boxed::Box, format, string::String, sync::Arc};
use core::{
    cell::UnsafeCell,
    fmt::{Debug, Display, LowerHex},
    marker::PhantomData,
    pin::Pin,
    sync::atomic::{AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering},
};

use super::{ProcessEntry, ThreadingError};
use crate::{
    arch::{
        self,
        context::{
            KTaskInfo,
            USER_STACK_SIZE,
            USER_STACK_START,
            UsrTaskInfo,
            allocate_kstack,
            allocate_userstack,
            copy_ustack_mappings_into,
            init_kernel_task,
            init_usr_task,
            unmap_ustack_mappings,
        },
        interrupt,
        mem::{PageSize, Size4KiB, VirtAddr},
    },
    kernel::{
        elf::apply,
        fd::{FDMap, File, FileDescriptor, MaybeOwned, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO},
        fs::{self, Path},
        mem::{
            align_up,
            paging::{APageTable, PAGETABLE, TaskPageTable, create_new_pagedir},
        },
        threading::{tls, trampoline::TaskExitInfo},
    },
    sync::locks::{Mutex, RwLock},
};

pub const USER_MMAP_START: usize = 0x9000_000_0000;

pub trait TaskRepr: Debug + Sized {
    fn tidx(&self) -> usize;
    fn tid(&self) -> ThreadID;
    fn pgrid(&self) -> ProcessGroupID;
    fn pid(&self) -> ProcessID;
    fn krsp(&self) -> VirtAddr;
    fn set_krsp(&self, addr: &VirtAddr);
    fn privilege(&self) -> PrivilegeLevel;
    #[allow(clippy::mut_from_ref)]
    fn pagedir(&self) -> &'static mut APageTable<'static>;
    fn state(&self) -> TaskState;
    fn set_state(&self, state: TaskState);
    fn state_data(&self) -> &Mutex<TaskStateData>;
    fn name(&self) -> Option<&str>;
    fn exit_info(&self) -> &TaskExitInfo;
    fn kstack_top(&self) -> &VirtAddr;
    fn fd(&self, descriptor: FileDescriptor) -> Option<Arc<File>>;
    fn add_fd(&self, descriptor: FileDescriptor, f: impl Into<Arc<File>>) -> Option<Arc<File>>;
    fn remove_fd(&self, descriptor: FileDescriptor) -> Option<Arc<File>>;
    fn add_next_file(&self, f: impl Into<Arc<File>>) -> FileDescriptor;
    fn next_fd(&self) -> FileDescriptor;
    fn next_addr(&self) -> &AtomicUsize;
    fn ensure_ready(self) -> Result<Self, ThreadingError>;
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrivilegeLevel {
    Kernel,
    User,
    #[default]
    Unset,
}

// a task represents a single thread in some process (where this thread may be the only one).
// Core contains data shared across threads in a process
// metadata contains data owned by each thread
// TaskID > 0 is globally unique and refers to a specific thread, which existed at some point

// TODO: we should replace MaybeOwned by an Arc<TaskCore>, and inititalize in another way
// Maybe not an Arc, but some type that we can enforce to be shared after taskBuilder::build() finishes.

#[derive(Debug)]
pub struct Task {
    pub metadata: TaskMetadata,
    pub core: MaybeOwned<TaskCore>,
}

#[derive(Debug)]
pub struct TaskCore {
    pub pagedir: UnsafeCell<APageTable<'static>>,
    pub heap_size: AtomicUsize,
    pub pid: ProcessID,
    pub pgrid: ProcessGroupID,
    pub fd_table: RwLock<FDMap>,
    pub next_free_addr: AtomicUsize,
    pub name: Option<String>,
    pub parent: Option<ThreadID>,
    pub state: AtomicU8,
    _private: PhantomData<()>,
}

unsafe impl Sync for TaskCore {}
unsafe impl Send for TaskCore {}

#[derive(Debug)]
pub struct TaskMetadata {
    pub state_data: Mutex<TaskStateData>,
    pub state: AtomicU8,
    pub exit_info: Pin<Box<TaskExitInfo>>,
    pub tid: ThreadID,
    pub tidx: AtomicUsize,
    pub user_stack_top: Option<VirtAddr>,
    pub ursp: Option<AtomicU64>,
    pub krsp: AtomicU64,
    pub kernel_stack_top: VirtAddr,
    pub privilege: PrivilegeLevel,
    _private: PhantomData<()>,
}

impl Task {
    fn new() -> Self {
        Self {
            metadata: TaskMetadata::new(),
            core: TaskCore::new().into(),
        }
    }
}

impl TaskCore {
    fn new() -> Self {
        let (pgrid, pid) = if let Some(current) = tls::task_data().current_thread()
            && let Some(group) = tls::task_data().current_pgr()
        {
            (current.pgrid(), group.read().next_pid())
        } else {
            (0.into(), 0.into())
        };

        Self {
            name: None,
            parent: tls::task_data()
                .current_thread()
                .map(|current| current.tid()),
            pid,   // copied from parent if thread
            pgrid, // copied from parent if exists or thread
            pagedir: APageTable::global().into(),
            heap_size: 0.into(),
            next_free_addr: AtomicUsize::new(0),
            fd_table: RwLock::default(),
            state: (TaskState::default() as u8).into(),
            _private: PhantomData,
        }
    }

    fn with_fd_table(mut self, table: FDMap) -> Self {
        self.fd_table = table.into();
        self
    }

    fn with_next_free(self, addr: usize) -> Self {
        self.next_free_addr.store(addr, Ordering::Relaxed);
        self
    }

    fn with_name(mut self, name: String) -> Self {
        self.name.replace(name);
        self
    }

    pub fn get_process_state(&self) -> TaskState {
        self.state.load(Ordering::Acquire).into()
    }

    pub fn set_process_state(&self, state: TaskState) {
        self.state.store(state as u8, Ordering::Release);
    }
}

impl TaskMetadata {
    fn new() -> Self {
        Self {
            state_data: TaskStateData::default().into(),
            exit_info: Box::pin(TaskExitInfo::default()),
            state: (TaskState::default() as u8).into(),
            privilege: PrivilegeLevel::default(),
            tid: get_tid(),
            tidx: 0.into(),
            krsp: 0.into(),
            kernel_stack_top: VirtAddr::zero(),
            user_stack_top: None,
            ursp: None,
            _private: PhantomData,
        }
    }

    fn with_state(mut self, data: TaskStateData) -> Self {
        self.state_data = data.into();
        self
    }
}

impl TaskRepr for Task {
    fn tid(&self) -> ThreadID {
        self.metadata.tid
    }

    fn tidx(&self) -> usize {
        self.metadata.tidx.load(Ordering::Relaxed)
    }

    fn pgrid(&self) -> ProcessGroupID {
        self.core.pgrid
    }

    fn krsp(&self) -> VirtAddr {
        VirtAddr::new(self.metadata.krsp.load(Ordering::Relaxed))
    }

    fn set_krsp(&self, addr: &VirtAddr) {
        self.metadata.krsp.store(addr.as_u64(), Ordering::Relaxed);
    }

    fn privilege(&self) -> PrivilegeLevel {
        self.metadata.privilege
    }

    // SAFTEY: This operation is safe IFF APageTable ownes ONLY locked types / shared refs.
    // Further, APageTable does NOT really live for 'static, it will be deallocated, once Task gets cleaned up.
    // Thus we must ensure, that no references remain (to be used) on cleanup.
    // Since the task will not be picked up after cleanup, this is only possible to occur, if some other task tries to use this tasks pagedir.
    // This should not be done anyways.
    // TODO: this should probably be marked as unsafe
    fn pagedir(&self) -> &'static mut APageTable<'static> {
        unsafe { &mut *self.core.pagedir.get() }
    }

    fn state(&self) -> TaskState {
        self.metadata.state.load(Ordering::Acquire).into()
    }

    fn set_state(&self, state: TaskState) {
        self.metadata.state.store(state as u8, Ordering::Release);
    }

    fn state_data(&self) -> &Mutex<TaskStateData> {
        &self.metadata.state_data
    }

    fn name(&self) -> Option<&str> {
        self.core.name.as_deref()
    }

    fn exit_info(&self) -> &TaskExitInfo {
        self.metadata.exit_info.as_ref().get_ref()
    }

    fn kstack_top(&self) -> &VirtAddr {
        &self.metadata.kernel_stack_top
    }

    fn fd(&self, descriptor: FileDescriptor) -> Option<Arc<File>> {
        self.core.fd_table.read().get(&descriptor).cloned()
    }

    /// inserts a K, V pair into fd table. If K was present, old V is returned in Some
    fn add_fd(&self, descriptor: FileDescriptor, f: impl Into<Arc<File>>) -> Option<Arc<File>> {
        self.core.fd_table.write().insert(descriptor, f.into())
    }

    fn remove_fd(&self, descriptor: FileDescriptor) -> Option<Arc<File>> {
        self.core.fd_table.write().remove(&(descriptor as u32))
    }

    fn add_next_file(&self, f: impl Into<Arc<File>>) -> FileDescriptor {
        let next_fd = self.next_fd();
        self.add_fd(next_fd, f);
        next_fd
    }

    fn next_fd(&self) -> FileDescriptor {
        self.core
            .fd_table
            .read()
            .last_key_value()
            .map(|(k, _)| *k + 1)
            .unwrap_or(0)
    }

    fn next_addr(&self) -> &AtomicUsize {
        &self.core.next_free_addr
    }

    fn ensure_ready(mut self) -> Result<Self, ThreadingError> {
        self.core = self.core.into_shared();
        Ok(self)
    }

    fn pid(&self) -> ProcessID {
        self.core.pid
    }
}

// in principle Task is Send + Sync, however care has to be taken, that fields such as nmae are properly synchronized. Might lock this.
unsafe impl Send for Task {}
unsafe impl Sync for Task {}

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
        F: FnOnce() + 'static + Send,
    {
        let boxed: Box<dyn FnOnce() + Send + 'static> = Box::new(func);
        let ptr = Box::new(boxed);
        Self::from_ptr(Box::into_raw(ptr))
    }

    pub unsafe fn as_val<T>(&self) -> T {
        let boxed = unsafe { Box::from_raw(self.0 as *mut T) };
        *boxed
    }

    pub unsafe fn as_closure(&self) -> Box<dyn FnOnce() + 'static + Send> {
        unsafe { *Box::from_raw(self.0 as *mut Box<dyn FnOnce() + 'static + Send>) }
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
#[allow(unused_mut)]
macro_rules! args {
    ($($arg:expr),* $(,)?) => {{
        const MAX_ARGS: usize = 6;
        #[allow(unused_mut)]
        let mut arr = [$crate::kernel::threading::task::Arg::default(); MAX_ARGS];
        #[allow(unused_mut, unused_assignments)]
        let mut idx = 0;
        $(
            if idx < MAX_ARGS {
                arr[idx] = $crate::kernel::threading::task::Arg::from_val($arg);
                #[allow(unused_asignments)]
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

#[allow(clippy::derivable_impls)]
impl Default for Init<'_> {
    fn default() -> Self {
        Self { elf_data: None }
    }
}

#[allow(dead_code)]
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
    _phatom: PhantomData<&'a ()>,
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

impl<S> TaskBuilder<Task, S> {
    pub fn with_name(mut self, name: String) -> TaskBuilder<Task, S> {
        self.inner.core.try_mut().unwrap().name.replace(name);
        self
    }

    pub fn with_exit_info(mut self, exit_info: TaskExitInfo) -> TaskBuilder<Task, S> {
        *self.inner.metadata.exit_info = exit_info;
        self
    }

    pub fn with_file(self, fd: FileDescriptor, f: File) -> TaskBuilder<Task, S> {
        _ = self.inner.add_fd(fd, f);
        self
    }

    /// adds open files of current into the new process, if current is accessible, else uses defaults for stdin, stderr and stdout
    pub fn with_default_files(self, clone_these: bool) -> TaskBuilder<Task, S> {
        if clone_these && let Some(current) = tls::task_data().current_thread() {
            self.override_files(
                current
                    .core
                    .fd_table
                    .read()
                    .iter()
                    .map(|(&fd, f)| (fd, f.clone())),
            )
        } else {
            let stdin =
                fs::open(Path::new("/proc/kernel/io/keyboard"), fs::OpenOptions::READ).unwrap();
            let stdout = fs::open(
                Path::new("/proc/kernel/io/fbbackend"),
                fs::OpenOptions::READ | fs::OpenOptions::WRITE,
            )
            .unwrap();
            let stderr = fs::open(
                Path::new("/proc/kernel/io/serial"),
                fs::OpenOptions::READ | fs::OpenOptions::WRITE,
            )
            .unwrap();

            self.override_files(
                [
                    (STDIN_FILENO, stdin.into()),
                    (STDOUT_FILENO, stdout.into()),
                    (STDERR_FILENO, stderr.into()),
                ]
                .into_iter(),
            )
        }
    }

    pub fn override_files(
        self,
        files: impl Iterator<Item = (FileDescriptor, Arc<File>)>,
    ) -> TaskBuilder<Task, S> {
        let mut table = self.inner.core.fd_table.write();
        for (fd, f) in files {
            table
                .entry(fd)
                .and_modify(|v| *v = f.clone())
                .or_insert(f.clone());
        }
        drop(table);
        self
    }
}

impl TaskBuilder<Task, Uninit> {
    pub unsafe fn from_addr<'a>(
        addr: VirtAddr,
    ) -> Result<TaskBuilder<Task, Init<'a>>, ThreadingError> {
        Ok(TaskBuilder::<Task, Init> {
            inner: Task::new(),
            entry: addr,
            data: TaskData::default(),
            _marker: Init::default(),
        })
    }

    pub fn from_fn<'a>(func: ProcessEntry) -> Result<TaskBuilder<Task, Init<'a>>, ThreadingError> {
        Ok(TaskBuilder::<Task, Init> {
            inner: Task::new(),
            entry: VirtAddr::new(func as usize as u64),
            data: TaskData::default(),
            _marker: Init::default(),
        })
    }

    pub fn from_bytes<'data>(
        bytes: &'data [u8],
    ) -> Result<TaskBuilder<Task, Init<'data>>, ThreadingError> {
        Ok(TaskBuilder::<Task, Init> {
            inner: Task::new(),
            entry: VirtAddr::zero(),
            data: TaskData::default(),
            _marker: Init::new(bytes),
        })
    }
}

impl TaskBuilder<Task, Init<'_>> {
    pub fn as_kernel(mut self) -> Result<TaskBuilder<Task, Ready<KTaskInfo>>, ThreadingError> {
        let stack_top = allocate_kstack()?;
        self.inner
            .metadata
            .krsp
            .store(stack_top.as_u64(), Ordering::Relaxed);
        self.inner.metadata.kernel_stack_top = stack_top;
        self.inner.metadata.privilege = PrivilegeLevel::Kernel;
        let info = KTaskInfo::new(
            self.entry,
            VirtAddr::new(self.inner.metadata.krsp.load(Ordering::Relaxed)),
        );
        Ok(TaskBuilder {
            inner: self.inner,
            entry: self.entry,
            data: self.data,
            _marker: info.into(),
        })
    }

    pub fn as_usr<'a>(
        mut self,
    ) -> Result<TaskBuilder<Task, Ready<ExtendedUsrTaskInfo<'a>>>, ThreadingError> {
        let kstack = allocate_kstack()?;
        let tbl = create_new_pagedir::<'a, '_>().map_err(|e| ThreadingError::PageDirNotBuilt)?;
        let mut tbl = APageTable::owned(tbl.into());

        let usr_end = allocate_userstack(&mut tbl, USER_STACK_START.align_up(Size4KiB::SIZE))?;

        self.inner
            .metadata
            .krsp
            .store(kstack.as_u64(), Ordering::Relaxed);
        self.inner.metadata.kernel_stack_top = kstack;

        self.inner.metadata.user_stack_top.replace(usr_end);
        self.inner
            .metadata
            .ursp
            .replace(AtomicU64::new(usr_end.as_u64()));

        self.inner.metadata.privilege = PrivilegeLevel::User;
        self.inner
            .core
            .next_free_addr
            .store(USER_MMAP_START, Ordering::Relaxed);

        if let Some(data) = self._marker.elf_data {
            let bytes = elf::ElfBytes::minimal_parse(data)
                .map_err(|e| ThreadingError::Unknown(format!("{:#?}", e)))?;
            self.entry = VirtAddr::new(bytes.ehdr.e_entry);
            apply(&bytes, data, &mut tbl)
                .map_err(|e| ThreadingError::Unknown(format!("{:#?}", e)))?;
        }

        let info = UsrTaskInfo::new(
            self.entry,
            VirtAddr::new(self.inner.metadata.krsp.load(Ordering::Relaxed)),
            usr_end,
            tbl.try_get_owned().unwrap().lock().root.start_address(),
        );

        let _marker = ExtendedUsrTaskInfo {
            info,
            _phatom: PhantomData,
        }
        .into();

        unsafe {
            self.inner.core.try_mut().unwrap().pagedir.replace(tbl);
        }

        Ok(TaskBuilder {
            inner: self.inner,
            entry: self.entry,
            data: self.data,
            _marker,
        })
    }

    pub fn like_existing_usr<'a>(
        mut self,
        task: &Task,
    ) -> Result<TaskBuilder<Task, Ready<ExtendedUsrTaskInfo<'a>>>, ThreadingError> {
        let core = task
            .core
            .try_clone()
            .ok_or(ThreadingError::Unknown("The tasks core is owned".into()))?;
        let next_idx = task.metadata.tidx.fetch_add(1, Ordering::Release);

        self.inner.core = core;
        self.inner.metadata.tidx.store(next_idx, Ordering::Relaxed);

        let kstack = allocate_kstack()?;
        let tbl = self.inner.pagedir();

        let usr_end = allocate_userstack(
            tbl,
            USER_STACK_START.align_up(Size4KiB::SIZE)
                + (next_idx * align_up(USER_STACK_SIZE, Size4KiB::SIZE as usize)) as u64,
        )?;

        self.inner.metadata.krsp = AtomicU64::new(kstack.as_u64());
        self.inner.metadata.kernel_stack_top = kstack;

        self.inner.metadata.user_stack_top.replace(usr_end);
        self.inner
            .metadata
            .ursp
            .replace(AtomicU64::new(usr_end.as_u64()));

        let info = UsrTaskInfo::new(
            self.entry,
            kstack,
            usr_end,
            tbl.try_get_owned()
                .ok_or(ThreadingError::Unknown(
                    "Pagetable not owned taskTable".into(),
                ))?
                .lock()
                .root
                .start_address(),
        );

        let _marker = ExtendedUsrTaskInfo {
            info,
            _phatom: PhantomData,
        }
        .into();

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
        unsafe {
            interrupt::disable();
        }

        copy_ustack_mappings_into(self.inner.pagedir(), &mut *PAGETABLE.lock());

        let next_top =
            unsafe { init_usr_task(&self._marker.inner.info, self.inner.exit_info(), &self.data) };

        unmap_ustack_mappings(&mut PAGETABLE.lock());

        unsafe {
            interrupt::enable();
        }

        self.inner.set_krsp(&next_top);
        // transmute core into a shared ptr, in order to
        // a) make fields immutable
        // b) allow threads with same core
        // After this point no exculsive refs to task are possible anyways
        self.inner.ensure_ready().unwrap()
    }
}

impl<T: TaskRepr> TaskBuilder<T, Ready<KTaskInfo>> {
    pub fn build(self) -> T {
        let next_top =
            unsafe { init_kernel_task(&self._marker.inner, self.inner.exit_info(), &self.data) };
        self.inner.set_krsp(&next_top);
        self.inner.ensure_ready().unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TaskState {
    Running,
    #[default]
    Ready,
    Blocking,
    Sleeping,
    Zombie,
}

impl TaskState {
    pub fn new() -> Self {
        Self::Ready
    }
}

impl From<u8> for TaskState {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Running,
            1 => Self::Ready,
            2 => Self::Blocking,
            3 => Self::Sleeping,
            4 => Self::Zombie,
            _ => panic!("invalid enum variant"),
        }
    }
}

impl From<&AtomicU8> for TaskState {
    fn from(value: &AtomicU8) -> Self {
        value.load(Ordering::Relaxed).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskStateData {
    Exit(ExitInfo),
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitInfo {
    pub exit_code: u32,
    pub signal: Option<u8>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Copy, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ThreadID {
    inner: u64,
}

impl ThreadID {
    pub fn new() -> Self {
        get_tid()
    }

    pub fn get_inner(&self) -> u64 {
        self.inner
    }
}

impl From<u64> for ThreadID {
    fn from(value: u64) -> Self {
        Self { inner: value }
    }
}

impl From<AtomicU64> for ThreadID {
    fn from(value: AtomicU64) -> Self {
        value.load(Ordering::Acquire).into()
    }
}

impl From<&AtomicU64> for ThreadID {
    fn from(value: &AtomicU64) -> Self {
        value.load(Ordering::Acquire).into()
    }
}

impl Display for ThreadID {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "TaskID {{ {} }}", self.inner)
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Default)]
pub struct ProcessID(pub u64);

impl From<u64> for ProcessID {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Default)]
pub struct ProcessGroupID(pub u64);

impl From<u64> for ProcessGroupID {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

pub fn get_tid() -> ThreadID {
    // PIDs start at 1 since locks use 0 as default value for "held by thread x"
    static CURRENT_PID: AtomicU64 = AtomicU64::new(1);
    let current = CURRENT_PID.fetch_add(1, Ordering::Relaxed);
    ThreadID { inner: current }
}

#[cfg(feature = "test_run")]
mod tests {
    use os_macros::{kernel_test, with_default_args};

    use super::*;
    use crate::kernel::threading::{ProcessReturn, spawn_fn};

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
        }

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
