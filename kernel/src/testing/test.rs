//TODO

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
