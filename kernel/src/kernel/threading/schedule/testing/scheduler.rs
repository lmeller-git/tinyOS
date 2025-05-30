use super::TestRunner as SuperTestRunner;
use tiny_os_common::{
    log,
    testing::{TestCase, TestRunner},
};
/// runs each Test in a separate thread and reports its outcome
/// no preemptive multitasking
pub struct SimpleTestRunner {}

impl SuperTestRunner for SimpleTestRunner {
    fn new() -> Self {
        Self {}
    }

    fn notify_panic(&self, info: &core::panic::PanicInfo) {
        log!("[ERR]")
        // switch back to original stack
    }
}

impl TestRunner for SimpleTestRunner {
    fn run_guarded(
        &self,
        task: extern "C" fn(),
        config: &tiny_os_common::testing::TestConfig,
        name: &str,
    ) {
        if !config.verbose {
            //TODO disble logging
        }

        match self.run(task) {
            Ok(()) => {}
            Err(e) => log!("\n[ERR] test {} could not be run: {:?}\n", name, e),
        };

        log!("[OK]\n")
    }
}
