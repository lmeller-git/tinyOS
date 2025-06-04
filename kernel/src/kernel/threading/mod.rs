use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::{string::String, sync::Arc};
use os_macros::kernel_test;
use schedule::{
    GLOBAL_SCHEDULER, GlobalTaskPtr, OneOneScheduler, add_built_task, add_ktask, add_task_ptr__,
    context_switch_local, get_unchecked,
};
use task::{ExitInfo, TaskBuilder, TaskState};
use trampoline::TaskExitInfo;

use crate::{arch, serial_println};

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
    task: Option<GlobalTaskPtr>,
}

impl JoinHandle {
    pub fn wait(&self) -> Result<usize, ThreadingError> {
        while !(self.inner.finished() || !self.is_task_alive().is_some_and(|v| v)) {
            // serial_println!(
            //     "yielding, {:#?}",
            //     self.task.as_ref().unwrap().raw().read().state
            // );
            yield_now();
        }
        // serial_println!("finished");
        Ok(
            self.inner.get_return().unwrap_or_else(|_| {
                if let TaskState::Zombie(ExitInfo {
                    exit_code,
                    signal: _,
                }) = self.task.as_ref().unwrap().raw().read().state
                {
                    exit_code as usize
                } else {
                    unreachable!()
                }
            }), // .ok_or(ThreadingError::Unknown("no return value".into()))
        )
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

#[derive(Debug, Default)]
struct RawJoinHandle {
    finished: AtomicBool,
    val: AtomicUsize,
}

impl RawJoinHandle {
    fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    fn get_return(&self) -> Result<usize, ThreadingError> {
        self.finished()
            .then_some(self.val.load(Ordering::Acquire))
            .ok_or(ThreadingError::Unknown("task not finished".into()))
    }
}

pub fn spawn_fn(func: extern "C" fn() -> usize) -> Result<JoinHandle, ThreadingError> {
    let mut handle = JoinHandle::default();
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
                raw.val.store(v, Ordering::Release);
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

#[kernel_test]
fn test() {
    assert_eq!(42, 42)
}
