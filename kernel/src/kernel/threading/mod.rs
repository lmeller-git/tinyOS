use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::{string::String, sync::Arc};
use schedule::{
    GLOBAL_SCHEDULER, OneOneScheduler, add_built_task, add_ktask, context_switch_local,
};
use task::{TaskBuilder, TaskState};
use trampoline::TaskExitInfo;

use crate::arch;

pub mod context;
pub mod schedule;
pub mod task;
pub mod trampoline;

pub fn init() {
    schedule::init();
}

#[derive(Debug)]
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

#[derive(Default, Debug, Clone)]
pub struct JoinHandle {
    inner: Arc<RawJoinHandle>,
}

impl JoinHandle {
    pub fn wait(&self) -> Result<usize, ThreadingError> {
        while !self.inner.finished() {
            yield_now();
        }
        Ok(
            self.inner.get_return(), // .ok_or(ThreadingError::Unknown("no return value".into()))
        )
    }
}

#[derive(Debug, Default)]
struct RawJoinHandle {
    finished: AtomicBool,
    val: AtomicUsize,
}

impl RawJoinHandle {
    fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    fn get_return(&self) -> usize {
        // self.val.map(|v| v.load(Ordering::Relaxed))
        self.val.load(Ordering::Acquire)
    }
}

pub fn spawn_fn(func: extern "C" fn() -> usize) -> Result<JoinHandle, ThreadingError> {
    let handle = JoinHandle::default();
    let raw = handle.inner.clone();

    let task = TaskBuilder::from_fn(func)?
        .as_kernel()?
        .with_exit_info(TaskExitInfo::new_with_default_trampoline(
            move |v: usize| {
                unsafe { GLOBAL_SCHEDULER.get_unchecked() }
                    .lock()
                    .current_mut()
                    .as_mut()
                    .map(|c| {
                        c.state = TaskState::Zombie(task::ExitInfo {
                            exit_code: v as u32,
                            signal: None,
                        })
                    });
                raw.val.store(v, Ordering::Release);
                raw.finished.store(true, Ordering::Release);
            },
        ))
        .build();

    add_built_task(task);
    Ok(handle)
}
