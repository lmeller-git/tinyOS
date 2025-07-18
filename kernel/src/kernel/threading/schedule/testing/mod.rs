mod scheduler;

use crate::{
    arch::context::switch_and_apply,
    kernel::threading::{
        ThreadingError,
        task::TaskBuilder,
        trampoline::{TaskExitInfo, test_kernel_return_trampoline},
    },
};
use conquer_once::spin::OnceCell;
use core::panic::PanicInfo;
pub use scheduler::*;

pub trait TestRunner {
    fn new() -> Self;
    fn run(&self, func: extern "C" fn() -> usize) -> Result<(), ThreadingError> {
        // let task = TaskBuilder::from_fn(func)?
        // .as_kernel()?
        // .with_exit_info(TaskExitInfo::new(next_test, test_kernel_return_trampoline))
        // .build();
        // unsafe { switch_and_apply(&task) };
        // Ok(())
        todo!()
    }
    fn notify_panic(&self, info: &PanicInfo);
}

pub extern "C" fn next_test() {
    //TODO
    unsafe {
        GLOBAL_TEST_SCHEDULER.get_unchecked();
    }
}

type GlobalTestScheduler = scheduler::SimpleTestRunner;

pub static GLOBAL_TEST_SCHEDULER: OnceCell<GlobalTestScheduler> = OnceCell::uninit();

pub fn init() {
    _ = GLOBAL_TEST_SCHEDULER.try_init_once(|| GlobalTestScheduler::new());
}
