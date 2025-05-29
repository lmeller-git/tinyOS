mod scheduler;

use core::panic::PanicInfo;

pub use scheduler::*;

use crate::{
    arch::context::switch_and_apply,
    kernel::threading::{ThreadingError, task::TaskBuilder},
};

pub trait TestRunner {
    fn new() -> Self;
    fn run(&mut self, func: extern "C" fn()) -> Result<(), ThreadingError> {
        let task = TaskBuilder::from_fn(func)?.as_kernel()?.build();
        unsafe { switch_and_apply(&task) };
        Ok(())
    }
    fn notify_panic(&self, info: &PanicInfo);
}

type GlobalTestScheduler = scheduler::SimpleTestRunner;

pub static GLOBAL_TEST_SCHEDULER: OnceCell<Mutex<GlobalTestScheduler>> = OnceCell::uninit();

pub fn init() {
    _ = GLOBAL_TEST_SCHEDULER.try_init_once(|| Mutex::new(GlobalTestScheduler::new()));
}
