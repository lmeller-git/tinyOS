pub mod logging;
pub mod serial;
use tiny_os_common::testing::{TestCase, TestConfig, TestRunner, TestingError, kernel::RawStr};

use crate::kernel::threading::ProcessEntry;

// #[cfg(feature = "test_run")]
#[repr(C)]
pub struct KernelTest {
    pub name: RawStr,
    pub func: ProcessEntry,
    pub config: TestConfig,
}

impl TestCase for KernelTest {
    fn name(&self) -> &str {
        self.name.to_str()
    }
    fn run_in(&self, runner: &dyn TestRunner) -> Result<(), TestingError> {
        runner.run_guarded(foo, &self.config, self.name());
        Ok(())
    }
}

extern "C" fn foo() -> usize {
    0
}

unsafe extern "C" {
    static __kernel_tests_start: KernelTest;
    static __kernel_tests_end: KernelTest;
}

#[allow(unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]
//SAFETY this is safe as long as __kernel_tests_start and end are properly defined in the linker and initialized correctly
pub unsafe fn get_kernel_tests() -> &'static [KernelTest] {
    let start = unsafe { &__kernel_tests_start as *const _ as usize };
    let end = unsafe { &__kernel_tests_end as *const _ as usize };
    let count = (end - start) / core::mem::size_of::<KernelTest>();
    core::slice::from_raw_parts(&__kernel_tests_start as *const _, count)
}
