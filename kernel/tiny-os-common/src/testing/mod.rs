pub trait TestCase {
    fn run(&self) {}
    fn name(&self) {}
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
}

impl<T> TestCase for T
where
    T: Fn(),
{
    fn run(&self) {
        self()
    }
}

#[cfg(feature = "test_run")]
pub mod tests {
    use super::*;
}
