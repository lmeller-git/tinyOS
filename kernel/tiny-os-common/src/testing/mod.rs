pub mod kernel;

pub trait TestCase {
    fn run(&self) {}
    fn run_in(&self, _runner: &dyn TestRunner) -> Result<(), TestingError> {
        Ok(())
    }
    fn name(&self) -> &str;
}

pub enum TestingError {}

pub trait TestRunner {
    fn run_guarded(&self, task: extern "C" fn() -> usize, config: &TestConfig, name: &str);
}

pub struct FileTestRunner {
    tests: &'static [&'static dyn TestCase],
}

impl FileTestRunner {
    pub fn new(tests: &'static [&'static dyn TestCase]) -> Self {
        Self { tests }
    }
}

impl TestCase for FileTestRunner {
    fn run(&self) {
        for test in self.tests {
            test.run()
        }
    }
    fn name(&self) -> &str {
        todo!()
    }
}

impl<T> TestCase for T
where
    T: Fn(),
{
    fn run(&self) {
        #[cfg(feature = "std")]
        ::std::print!("{}...", ::std::any::type_name::<Self>());
        #[cfg(not(feature = "std"))]
        crate::log!("{}...", ::core::any::type_name::<Self>());

        self();

        #[cfg(feature = "std")]
        ::std::println!("\t[OK]");
        #[cfg(not(feature = "std"))]
        crate::log!("\t[OK]\n");
    }

    fn name(&self) -> &str {
        todo!()
    }
}

#[repr(C)]
#[derive(Default)]
pub struct TestConfig {
    pub should_panic: bool,
    pub verbose: bool,
}
#[allow(unused_imports)]
#[cfg(feature = "test_run")]
pub mod tests {
    use super::*;
}
