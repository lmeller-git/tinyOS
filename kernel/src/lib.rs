#![no_std]

use core::panic::PanicInfo;
use os_macros;

#[cfg(feature = "test_run")]
mod testing;

#[cfg(feature = "test_run")]
const TESTS: &[&dyn testing::TestCase] = &[&test_add, &test_add2];

#[cfg(feature = "test_run")]
impl<T> testing::TestCase for T
where
    T: Fn(),
{
    fn run(&self) {
        self()
    }
}

#[cfg(feature = "test_run")]
pub fn test_main() {
    test_runner(TESTS);
    exit_qemu(QemuExitCode::Success);
}

#[cfg(feature = "test_run")]
pub fn test_runner(tests: &[&dyn testing::TestCase]) {
    for test in tests {
        test.run();
    }
}

#[cfg(feature = "test_run")]
pub fn test_panic_handler(info: &PanicInfo) -> ! {
    exit_qemu(QemuExitCode::Failed);
    loop {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

#[cfg(feature = "test_run")]
fn test_add() {
    assert_eq!(42, 42);
}

#[cfg(feature = "test_run")]
fn test_add2() {
    assert_eq!(42, 0);
}
