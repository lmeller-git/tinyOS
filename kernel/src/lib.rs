#![no_std]

use core::panic::PanicInfo;
use os_macros;

#[cfg(feature = "test_run")]
use os_macros::tests;
#[cfg(feature = "test_run")]
use tiny_os_common::testing::TestCase;

#[cfg(feature = "test_run")]
pub fn test_main() {
    test_runner();
    exit_qemu(QemuExitCode::Success);
}

#[cfg(feature = "test_run")]
pub fn test_runner() {
    tests::test_runner();
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
tests! {
    #[test_case]
    fn trivial() {
        let a = 0;
        assert_eq!(a, 0);
    }
    #[test_case]
    fn trivial_fail() {
        let a = 1;
        assert_eq!(a, 0);
    }
}
