#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

pub fn add(a: u8, b: u8) -> u8 {
    a + b
}

pub fn test_runner(tests: &[&dyn Fn()]) {
    for test in tests {
        test();
    }
}
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

#[cfg(test)]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    exit_qemu(QemuExitCode::Success);
    test_main();
    loop {}
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}

#[test_case]
fn add_() {
    assert_eq!(add(2, 1), 4);
}
