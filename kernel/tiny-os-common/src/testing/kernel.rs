use core::{convert::Infallible, str::FromStr};

use super::{TestCase, TestConfig};

#[repr(C)]
pub struct KernelTest {
    pub name: RawStr,
    pub func: extern "C" fn() -> (),
    pub config: TestConfig,
}

#[repr(C)]
pub struct RawStr {
    start: *const u8,
    len: usize,
}

impl RawStr {
    pub fn to_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.start, self.len)) }
    }

    pub const fn from_s_str(s: &'static str) -> Self {
        Self {
            start: s.as_ptr(),
            len: s.len(),
        }
    }
}

impl FromStr for RawStr {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            start: s.as_ptr(),
            len: s.len(),
        })
    }
}

unsafe impl Sync for RawStr {}
unsafe impl Send for RawStr {}

impl TestCase for KernelTest {
    fn name(&self) -> &str {
        self.name.to_str()
    }
    fn run_in(&self, runner: &mut dyn super::TestRunner) -> Result<(), super::TestingError> {
        runner.run_guarded(self.func, &self.config, self.name());
        Ok(())
    }
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
