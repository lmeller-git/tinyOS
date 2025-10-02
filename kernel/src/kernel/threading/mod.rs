use alloc::{boxed::Box, format, string::String, sync::Arc};
use core::{
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

use schedule::{GlobalTaskPtr, add_task_ptr__};
use task::{Arg, Args, TaskBuilder, TaskState};
use thiserror::Error;
use trampoline::{TaskExitInfo, closure_trampoline};

use crate::{
    args,
    drivers::wait_manager,
    kernel::{
        abi::syscalls::funcs::{sys_exit, sys_yield},
        threading::{
            task::TaskRepr,
            wait::{
                QueuTypeCondition,
                QueueHandle,
                QueueType,
                condition::WaitCondition,
                queues::GenericWaitQueue,
            },
        },
    },
    sync::locks::RwLock,
};

pub mod context;
pub mod schedule;
pub mod task;
pub mod tls;
pub mod trampoline;
pub mod wait;

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

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ThreadingError {
    #[error("the stack for a task could not be built")]
    StackNotBuilt,
    #[error("the stack of a task could not be deallocated")]
    StackNotFreed,
    #[error("the pagedir of a task could not be built")]
    PageDirNotBuilt,
    #[error("unspecified threading error:\n{0}")]
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
        if let Some(t) = &self.task
            && !(self.inner.finished() || !self.is_task_alive().is_some_and(|v| v))
        {
            wait_manager::add_queue(
                QueueHandle::from_owned(Box::new(GenericWaitQueue::new())),
                QueueType::Thread(t.pid()),
            );
        }

        let wait_conds = &[QueuTypeCondition::with_cond(
            QueueType::Thread(self.task.as_ref().map(|t| t.pid()).unwrap_or_default()),
            WaitCondition::Thread(self.task.as_ref().map(|t| t.pid()).unwrap_or_default()),
        )];

        while !(self.inner.finished() || !self.is_task_alive().is_some_and(|v| v)) {
            wait_manager::add_wait(&tls::task_data().current_pid(), wait_conds);
            yield_now();
        }

        if let Some(t) = &self.task {
            wait_manager::remove_queue(&QueueType::Thread(t.pid()));
        }

        let r = self.inner.get_return().map_err(|e| {
            if let TaskState::Zombie = self.task.as_ref().unwrap().state() {
                ThreadingError::Unknown(format!(
                    "task terminated with {:#?}",
                    &*self.task.as_ref().unwrap().state_data().lock()
                ))
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
            .map(|task| !matches!(task.state(), TaskState::Zombie))
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
                raw.finished.store(true, Ordering::Release);
                sys_exit(0)
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
    F: FnOnce() -> R + 'static + Send,
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
    use os_macros::{kernel_test, with_default_args};

    use super::*;
    use crate::args;

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
    fn spawn_fn_test() {
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
