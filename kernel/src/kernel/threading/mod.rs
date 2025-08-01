use crate::{
    arch, args,
    kernel::{
        abi::syscalls::funcs::sys_yield,
        threading::{schedule::with_current_task, task::TaskRepr},
    },
    serial_println,
    sync::locks::RwLock,
};
use alloc::{format, string::String, sync::Arc};
use core::{
    arch::asm,
    hint,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use os_macros::kernel_test;
use schedule::{
    GlobalTaskPtr, OneOneScheduler, add_built_task, add_ktask, add_task_ptr__,
    context_switch_local, current_task, with_scheduler,
};
use task::{Arg, Args, ExitInfo, TaskBuilder, TaskState};
use trampoline::{TaskExitInfo, closure_trampoline};

pub mod context;
pub mod schedule;
pub mod task;
pub mod tls;
pub mod trampoline;

pub type ProcessReturn = usize;
pub type ProcessEntry = extern "C" fn(Arg, Arg, Arg, Arg, Arg, Arg) -> ProcessReturn;

static IS_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init() {
    schedule::init();
}

pub fn finalize() {
    IS_INITIALIZED.store(true, Ordering::Relaxed);
}

pub fn is_running() -> bool {
    IS_INITIALIZED.load(Ordering::Relaxed)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ThreadingError {
    StackNotBuilt,
    StackNotFreed,
    PageDirNotBuilt,
    Unknown(String),
}

pub fn yield_now() {
    //TODO
    use crate::arch::interrupt;
    if interrupt::are_enabled() {
        sys_yield();
    } else {
        hint::spin_loop();
    }
}

#[derive(Debug, Clone)]
pub struct JoinHandle<R> {
    inner: Arc<RawJoinHandle<R>>,
    task: Option<GlobalTaskPtr>,
}

impl<R> JoinHandle<R> {
    pub fn wait(&self) -> Result<R, ThreadingError> {
        while !(self.inner.finished() || !self.is_task_alive().is_some_and(|v| v)) {
            yield_now();
        }

        let r = self.inner.get_return().map_err(|e| {
            if let TaskState::Zombie(ExitInfo {
                exit_code,
                signal: _,
            }) = self.task.as_ref().unwrap().read().state
            {
                ThreadingError::Unknown(format!("task terminated with {}", exit_code))
            } else {
                panic!("something unexpected happend. Error: {:#?}", e);
            }
        })?;
        Ok(r)
    }

    pub fn wait_while<F>(&self, f: F) -> Result<R, ThreadingError>
    where
        F: Fn(&JoinHandle<R>),
    {
        while !(self.inner.finished() || !self.is_task_alive().is_some_and(|v| v)) {
            f(self)
        }
        // this will finish immediately
        self.wait()
    }

    fn is_task_alive(&self) -> Option<bool> {
        self.task
            .as_ref()
            .map(|task| !matches!(task.read().state, TaskState::Zombie(_)))
    }

    pub fn attach(&mut self, ptr: GlobalTaskPtr) {
        self.task.replace(ptr);
    }

    pub fn get_task(&self) -> Option<GlobalTaskPtr> {
        self.task.clone()
    }
}

impl<R> Default for JoinHandle<R> {
    fn default() -> Self {
        Self {
            inner: Arc::new(RawJoinHandle::default()),
            task: None,
        }
    }
}
#[derive(Debug)]
struct RawJoinHandle<R> {
    finished: AtomicBool,
    val: RwLock<Option<R>>,
}

impl<R> RawJoinHandle<R> {
    fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    fn get_return(&self) -> Result<R, ThreadingError> {
        self.finished()
            .then_some(self.val.write().take())
            .flatten()
            .ok_or(ThreadingError::Unknown("task not finished".into()))
    }
}

impl<R> Default for RawJoinHandle<R> {
    fn default() -> Self {
        Self {
            finished: AtomicBool::new(false),
            val: RwLock::new(None),
        }
    }
}

pub fn spawn_fn(
    func: ProcessEntry,
    args: Args,
) -> Result<JoinHandle<ProcessReturn>, ThreadingError> {
    let mut handle: JoinHandle<ProcessReturn> = JoinHandle::default();
    let raw = handle.inner.clone();
    let task: GlobalTaskPtr = TaskBuilder::from_fn(func)?
        .with_args(args)
        .with_default_devices()
        .as_kernel()?
        .with_exit_info(TaskExitInfo::new_with_default_trampoline(
            move |v: usize| {
                raw.val.write().replace(v);
                with_current_task(|task| task.write().kill_with_code(v));
                raw.finished.store(true, Ordering::Release);
                yield_now();
            },
        ))
        .build()
        .into();

    handle.attach(task.clone());
    add_task_ptr__(task);
    Ok(handle)
}

pub fn spawn<F, R>(func: F) -> Result<JoinHandle<R>, ThreadingError>
where
    F: FnOnce() -> R + 'static + Send + Sync,
    R: Send + Sync + 'static,
{
    let mut handle: JoinHandle<R> = JoinHandle::default();
    let raw = handle.inner.clone();

    let wrapper = move || {
        let ret = func();
        raw.val.write().replace(ret);
        raw.finished.store(true, Ordering::Release);
    };

    let mut args = args!();
    *args.get_mut(0) = Arg::from_fn(wrapper);
    let _outer_handle = spawn_fn(closure_trampoline, args)?;
    if let Some(ptr) = _outer_handle.task {
        handle.attach(ptr);
    }

    Ok(handle)
}

#[cfg(feature = "test_run")]
mod tests {
    use super::*;
    use crate::args;
    use os_macros::with_default_args;

    #[kernel_test]
    fn join_handle() {
        let handle: JoinHandle<usize> = JoinHandle::default();
        let raw = handle.inner.clone();
        (move || {
            raw.val.write().replace(42);
            raw.finished.store(true, Ordering::Relaxed);
        })();
        assert_eq!(handle.wait(), Ok(42));

        let handle: JoinHandle<&str> = JoinHandle::default();
        let raw = handle.inner.clone();
        (move || {
            raw.val.write().replace("hello");
            raw.finished.store(true, Ordering::Relaxed);
        })();
        assert_eq!(handle.wait(), Ok("hello"));
    }

    #[with_default_args]
    extern "C" fn foo() -> ProcessReturn {
        42
    }

    #[with_default_args]
    extern "C" fn bar() -> ProcessReturn {
        0
    }

    #[kernel_test]
    fn spawn_fn_() {
        let handle = spawn_fn(foo, args!()).unwrap();
        let handle2 = spawn_fn(bar, args!()).unwrap();
        assert_eq!(handle.wait(), Ok(42));
        assert_eq!(handle2.wait(), Ok(0))
    }

    #[kernel_test]
    fn spawn_closure() {
        let hello = "hello world";
        let value = 42;
        let atomic = Arc::new(AtomicBool::new(false));
        let atomic_ptr = atomic.clone();
        assert_eq!(spawn(|| { 42 }).unwrap().wait(), Ok(42));
        assert_eq!(
            spawn(move || {
                atomic_ptr.store(true, Ordering::Relaxed);
                let new_value = format!("{}_{}", hello, value);
                new_value
            })
            .unwrap()
            .wait()
            .unwrap(),
            "hello world_42"
        );
        assert_eq!(atomic.load(Ordering::Relaxed), true);
    }
}
