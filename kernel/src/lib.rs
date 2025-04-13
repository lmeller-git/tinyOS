#![no_std]
#![feature(abi_x86_interrupt)]

pub extern crate alloc;

#[cfg(feature = "test_run")]
use core::panic::PanicInfo;

use alloc::vec::Vec;
//#[cfg(feature = "test_run")]
use os_macros::tests;
use thiserror::Error;
#[cfg(feature = "test_run")]
use tiny_os_common::testing::TestCase;

pub mod arch;
pub mod bootinfo;
pub mod kernel;
pub mod locks;
pub mod requests;
pub mod structures;

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

#[derive(Error, Debug)]
pub enum KernelError {}

// #[cfg(feature = "test_run")]
tests! {
    #[test_case]
    fn trivial() {
        let a = 0;
        assert_eq!(a, 0);
    }
    #[test_case]
    fn trivial_fail() {
        let a = 1;
        assert_eq!(a, 1);
    }
    #[test_case]
    fn test_locks() {
        locks::tests::test_runner.run();
    }

    #[test_case]
    fn t() {
        let mut v = Vec::new();
        v.push(42);
        assert_eq!(v[0], 42);
    }
}
