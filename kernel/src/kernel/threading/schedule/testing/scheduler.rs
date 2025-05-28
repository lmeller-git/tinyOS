use crate::kernel::threading::schedule::TestRunner;
use alloc::boxed::Box;
use tiny_os_common::testing::TestCase;

/// runs each Test in a separate thread and reports its outcome
/// no preemptive multitasking
pub struct SimpleTestRunner {
    tests: &'static [Box<dyn TestCase>],
}

impl TestRunner for SimpleTestRunner {}
