use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::{format, string::String, sync::Arc};
use os_macros::kernel_test;
use schedule::{
    GLOBAL_SCHEDULER, GlobalTaskPtr, OneOneScheduler, add_built_task, add_ktask, add_task_ptr__,
    context_switch_local, get_unchecked,
};
use spin::RwLock;
use task::{ExitInfo, TaskBuilder, TaskState};
use trampoline::{TaskExitInfo, closure_trampoline};

use crate::{arch, serial_println};

pub mod context;
pub mod schedule;
pub mod task;
pub mod trampoline;

pub fn init() {
    schedule::init();
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
    arch::timer();
}

#[derive(Debug, Clone)]
pub struct JoinHandle<R> {
    inner: Arc<RawJoinHandle<R>>,
    task: Option<GlobalTaskPtr>,
}

impl<R> JoinHandle<R> {
    pub fn wait(&self) -> Result<R, ThreadingError> {
        while !(self.inner.finished() || !self.is_task_alive().is_some_and(|v| v)) {
            // serial_println!(
            //     "yielding, {:#?}",
            //     self.task.as_ref().unwrap().raw().read().state
            // );
            yield_now();
        }
        // serial_println!("finished");
        let r = self.inner.get_return().map_err(|_| {
            if let TaskState::Zombie(ExitInfo {
                exit_code,
                signal: _,
            }) = self.task.as_ref().unwrap().raw().read().state
            {
                ThreadingError::Unknown(format!("task terminated with {}", exit_code))
            } else {
                unreachable!()
            }
        })?;
        Ok(r)
    }

    fn is_task_alive(&self) -> Option<bool> {
        self.task
            .as_ref()
            .map(|task| !matches!(task.raw().read().state, TaskState::Zombie(_)))
    }

    pub fn attach(&mut self, ptr: GlobalTaskPtr) {
        self.task.replace(ptr);
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

pub fn spawn_fn(func: extern "C" fn() -> usize) -> Result<JoinHandle<usize>, ThreadingError> {
    let mut handle: JoinHandle<usize> = JoinHandle::default();
    let raw = handle.inner.clone();

    let task: GlobalTaskPtr = TaskBuilder::from_fn(func)?
        .as_kernel()?
        .with_exit_info(TaskExitInfo::new_with_default_trampoline(
            move |v: usize| {
                // serial_println!("hello");
                unsafe { get_unchecked() }.current().map(|c| {
                    c.raw().write().state = TaskState::Zombie(task::ExitInfo {
                        exit_code: v as u32,
                        signal: None,
                    })
                });
                raw.val.write().replace(v);
                raw.finished.store(true, Ordering::Release);
                // serial_println!("hello 2");
                yield_now();
            },
        ))
        .build()
        .into();

    handle.attach(task.clone());
    add_task_ptr__(task);
    Ok(handle)
}

// pub fn spawn<F, R>(func: F) -> Result<JoinHandle, ThreadingError>
// where
//     F: FnOnce() -> R + 'static + Send + Sync,
// {
//     let mut handle: JoinHandle<R> = JoinHandle::default();
//     let raw = handle.inner.clone();

//     let task: GlobalTaskPtr = TaskBuilder::from_fn(closure_trampoline)?
//         .as_kernel()?
//         .with_exit_info(|| {});
// }

#[cfg(feature = "test_run")]
mod tests {
    use super::*;

    #[kernel_test]
    fn join_handle() {
        let handle: JoinHandle<usize> = JoinHandle::default();
        let raw = handle.inner.clone();
        (move || {
            raw.finished.store(true, Ordering::Relaxed);
            raw.val.write().replace(42);
        })();
        assert_eq!(handle.wait(), Ok(42));

        let handle: JoinHandle<&str> = JoinHandle::default();
        let raw = handle.inner.clone();
        (move || {
            raw.finished.store(true, Ordering::Relaxed);
            raw.val.write().replace("hello");
        })();
        assert_eq!(handle.wait(), Ok("hello"));
    }

    extern "C" fn foo() -> usize {
        42
    }

    extern "C" fn bar() -> usize {
        0
    }

    #[kernel_test]
    fn spawn_fn_() {
        let handle = spawn_fn(foo).unwrap();
        let handle2 = spawn_fn(bar).unwrap();
        assert_eq!(handle.wait(), Ok(42));
        assert_eq!(handle2.wait(), Ok(0))
    }
}
