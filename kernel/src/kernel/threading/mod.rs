use core::sync::atomic::{AtomicBool, Ordering};

use schedule::{add_built_task, add_ktask, context_switch_local};
use task::TaskBuilder;
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
}

pub fn yield_now() {
    //TODO
    arch::timer();
}

#[derive(Default, Debug)]
pub struct JoinHandle {
    finished: AtomicBool,
    val: Option<usize>,
}

pub fn spawn_fn(func: extern "C" fn() -> usize) -> Result<JoinHandle, ThreadingError> {
    let handle = JoinHandle::default();
    // let task = TaskBuilder::from_fn(func)?
    //     .as_kernel()?
    //     .with_exit_info(TaskExitInfo::new_with_default_trampoline(|v: usize| {
    //         handle.val = Some(v);
    //         handle.finished.store(true, Ordering::Relaxed);
    //     }))
    //     .build();
    // add_built_task(task);
    Ok(handle)
}
