use super::TestRunner as SuperTestRunner;
use tiny_os_common::{
    log,
    testing::{TestCase, TestRunner},
};
/// runs each Test in a separate thread and reports its outcome
/// no preemptive multitasking
//TODO: should have a Runner thread, which spawns new
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
        task: extern "C" fn() -> usize,
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
