#![no_std]
#![feature(abi_x86_interrupt)]
#![feature(result_flattening)]
#![allow(unused_imports, unreachable_code, unsafe_op_in_unsafe_fn)]
pub extern crate alloc;

#[cfg(feature = "test_run")]
use crate::kernel::threading::schedule::testing::{self, GLOBAL_TEST_SCHEDULER, TestRunner};
#[cfg(feature = "test_run")]
use alloc::vec::Vec;
use arch::hcf;
#[cfg(feature = "test_run")]
use core::panic::PanicInfo;
#[cfg(feature = "test_run")]
use kernel::threading::{self, JoinHandle, schedule::add_named_ktask, spawn_fn, yield_now};
use os_macros::{kernel_test, tests};
use thiserror::Error;
use tiny_os_common::testing::{TestCase, kernel::get_kernel_tests};

pub mod arch;
pub mod bootinfo;
pub mod common;
pub mod drivers;
pub mod kernel;
pub mod locks;
pub mod requests;
pub mod services;
pub mod structures;
pub mod term;

#[cfg(feature = "test_run")]
struct TestLogger {}
#[cfg(feature = "test_run")]
impl tiny_os_common::logging::Logger for TestLogger {
    fn log(&self, msg: ::core::fmt::Arguments) {
        serial_print!("{}", msg);
    }
}

#[cfg(feature = "test_run")]
pub fn test_main() {
    test_test_main();
    tiny_os_common::logging::set_logger(&TestLogger {});
    exit_qemu(QemuExitCode::Success);
}

#[cfg(feature = "test_run")]
pub fn test_test_main() {
    threading::init();
    testing::init();
    add_named_ktask(kernel_test_runner, "test runner".into());
    yield_now();
    // let tests = unsafe { get_kernel_tests() };
    // serial_println!("huhu");
    // for test in tests {
    // serial_println!("name: {}", test.name());
    // test.run_in(unsafe { GLOBAL_TEST_SCHEDULER.get_unchecked() });
    // }
}

#[cfg(feature = "test_run")]
extern "C" fn kernel_test_runner() -> usize {
    let tests = unsafe { get_kernel_tests() };
    serial_println!("running {} tests...", tests.len());
    let mut tests_failed = false;
    let max_len = tests.iter().map(|t| t.name().len()).max().unwrap_or(0);
    for test in tests {
        let dots = ".".repeat(max_len - test.name().len() + 3);
        serial_print!("{}{} ", test.name(), dots);
        match spawn_fn(test.func).map(|handle| handle.wait()).flatten() {
            Ok(v) => {
                if v == 0 && !test.config.should_panic {
                    serial_println!("\x1b[32m[OK]\x1b[0m");
                } else if test.config.should_panic && v != 0 {
                    serial_println!("\x1b[33m[OK]\x1b[0m");
                } else {
                    serial_println!("\x1b[31m[ERR]\x1b[0m");
                    tests_failed = true;
                }
            }
            Err(_) => {
                if test.config.should_panic {
                    serial_println!("\x1b[33m[OK]\x1b[0m");
                } else {
                    serial_println!("\x1b[1;31m[ERR]\x1b[0m");
                    tests_failed = true;
                }
            }
        };
    }
    exit_qemu(if tests_failed {
        QemuExitCode::Failed
    } else {
        QemuExitCode::Success
    });
    0
}

#[cfg(feature = "test_run")]
pub fn test_panic_handler(info: &PanicInfo) -> ! {
    use arch::hcf;
    use kernel::threading::{
        self,
        schedule::with_current_task,
        task::{ExitInfo, TaskState},
    };
    with_current_task(|task| {
        task.raw().write().state = TaskState::Zombie(ExitInfo {
            exit_code: 1,
            signal: None,
        })
    });
    threading::yield_now();
    hcf()
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

#[kernel_test(should_panic)]
fn should_panic_err() {
    // works
    return todo!();
    assert!(true)
}

#[kernel_test(should_panic)]
fn should_panic() {
    assert!(false)
}

#[kernel_test]
fn correct() {
    assert!(true)
}
