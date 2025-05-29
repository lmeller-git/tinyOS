use alloc::boxed::Box;
use tiny_os_common::testing::{TestCase, TestRunner};

/// runs each Test in a separate thread and reports its outcome
/// no preemptive multitasking
pub struct SimpleTestRunner {
    tests: &'static [Box<dyn TestCase>],
}

impl crate::kernel::threading::schedule::TestRunner for SimpleTestRunner {}

impl TestRunner for SimpleTestRunner {
    fn run_guarded(
        &mut self,
        task: extern "C" fn(),
        config: &tiny_os_common::testing::TestConfig,
        name: &str,
    ) {
        todo!()
    }
}
