use super::{
    schedule::{GLOBAL_SCHEDULER, OneOneScheduler, context_switch_local},
    task::TaskRepr,
};
use crate::arch::hcf;
use core::arch::asm;

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe extern "C" fn kernel_return_trampoline(rsp: u64, returnto: u64) {
    // addr of this is set as the return address for tasks
    // rsp is currently at the topmost addr of tasks stack
    // should:
    // restore cpu context
    // call correct next func

    asm!("mov rsp, rdi", "mov rdi, rsi")
}

pub extern "C" fn default_exit() -> ! {
    if let Some(ref mut sched) = GLOBAL_SCHEDULER.get().map(|sched| sched.lock()) {
        if let Some(current) = sched.current_mut() {
            //TODO kill with info
            current.kill();
        }
    }
    unsafe { context_switch_local(0) };
    hcf();
}

#[derive(Debug)]
#[repr(C)]
pub struct TaskExitInfo {
    returnto: u64,
    trampoline: u64,
}

impl Default for TaskExitInfo {
    fn default() -> Self {
        Self {
            returnto: default_exit as usize as u64,
            trampoline: kernel_return_trampoline as usize as u64,
        }
    }
}
