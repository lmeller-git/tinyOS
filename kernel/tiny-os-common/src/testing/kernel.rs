use super::{TestCase, TestConfig};

pub struct KernelTest {
    name: &'static str,
    func: extern "C" fn() -> (),
    config: TestConfig,
}

impl TestCase for KernelTest {}
