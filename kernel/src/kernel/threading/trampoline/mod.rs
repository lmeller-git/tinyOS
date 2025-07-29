use alloc::{boxed::Box, sync::Arc};
use os_macros::with_default_args;

use super::{
    ProcessReturn,
    schedule::{OneOneScheduler, context_switch_local},
    task::{Arg, TaskRepr},
};
use crate::{
    arch::{context::return_trampoline_stub, hcf},
    kernel::threading::schedule::with_current_task,
    serial_println,
};
use core::{arch::asm, fmt::Debug, pin::Pin};

#[unsafe(no_mangle)]
#[with_default_args]
pub extern "C" fn closure_trampoline(func: Arg) -> ProcessReturn {
    let func = unsafe { func.as_closure() };
    (func)();
    ProcessReturn::default()
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_return_trampoline(ret: ProcessReturn, info: &mut TaskExitInfo) {
    // addr of this is set as the return address for tasks
    // rsp is currently at the topmost addr of tasks stack
    // should:
    // restore cpu context
    // call correct next func
    // just stay on tasks stack
    (info.callback.inner)(ret);
}

#[cfg(feature = "test_run")]
#[unsafe(no_mangle)]
pub extern "C" fn test_kernel_return_trampoline(ret: ProcessReturn, returnto: extern "C" fn()) {
    returnto();
}

pub fn default_exit(ret: usize) {
    serial_println!("default exit");
    with_current_task(|task| task.write_inner().kill_with_code(ret));
    unsafe { context_switch_local(0) };
    hcf();
}

#[repr(C)]
pub struct Callback {
    pub inner: Box<dyn Fn(ProcessReturn) + Send + Sync + 'static>,
}

impl Callback {
    pub fn new<F>(func: F) -> Self
    where
        F: Fn(ProcessReturn) + Send + Sync + 'static,
    {
        Self {
            inner: Box::new(func),
        }
    }
}

#[repr(C)]
pub struct TaskExitInfo {
    pub trampoline: u64,
    pub callback: Callback,
}

impl Default for TaskExitInfo {
    fn default() -> Self {
        Self {
            callback: Callback::new(default_exit),
            trampoline: kernel_return_trampoline as usize as u64,
        }
    }
}

impl Debug for TaskExitInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        _ = writeln!(f, "trampoline: {:#x}", self.trampoline);
        Ok(())
    }
}

impl TaskExitInfo {
    pub fn new<F>(callback: F, trampoline: extern "C" fn()) -> Self
    where
        F: Fn(ProcessReturn) + Send + Sync + 'static,
    {
        Self {
            callback: Callback::new(callback),
            trampoline: trampoline as usize as u64,
        }
    }

    pub fn new_with_default_trampoline<F>(callback: F) -> Self
    where
        F: Fn(ProcessReturn) + Send + Sync + 'static,
    {
        Self {
            callback: Callback::new(callback),
            trampoline: kernel_return_trampoline as usize as u64,
        }
    }
}
