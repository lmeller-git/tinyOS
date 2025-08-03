#![no_std]
#![feature(abi_x86_interrupt)]
#![feature(lazy_type_alias)]
#![feature(unsafe_cell_access)]
#![feature(stmt_expr_attributes)]
#![allow(
    unreachable_code,
    unused_doc_comments,
    unused_variables,
    private_interfaces,
    incomplete_features,
    clippy::missing_safety_doc
)]
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
use kernel::{
    devices::{DeviceBuilder, FdEntry, GraphicsTag, SinkTag, StdErrTag, StdInTag, TaskDevices},
    threading::task::{Arg, TaskRepr},
};
use os_macros::{kernel_test, tests, with_default_args};
use thiserror::Error;
use tiny_os_common::testing::TestCase;
pub use utils::*;

pub mod arch;
pub mod bootinfo;
pub mod common;
pub mod drivers;
pub mod include_bins;
pub mod kernel;
pub mod requests;
pub mod services;
pub mod structures;
pub mod term;
mod utils;

#[cfg(feature = "test_run")]
const MAX_TEST_TIME: u64 = 10000;

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
pub fn test_test_main() -> ! {
    use arch::interrupt::enable_threading_interrupts;
    use drivers::start_drivers;
    use kernel::threading;

    threading::init();
    testing::init();
    with_devices!(
        |devices| {
            let out: FdEntry<SinkTag> = DeviceBuilder::tty().serial();
            let input: FdEntry<StdInTag> = DeviceBuilder::tty().keyboard();
            let gfx: FdEntry<GraphicsTag> = DeviceBuilder::gfx().simple();
            devices.attach(out);
            devices.attach(input);
            devices.attach(gfx);
        },
        || { add_named_ktask(kernel_test_runner, "test runner".into()) }
    );
    start_drivers();
    threading::finalize();

    enable_threading_interrupts();
    yield_now();
    unreachable!()
}

use kernel::threading::ProcessReturn;
#[cfg(feature = "test_run")]
#[with_default_args]
extern "C" fn kernel_test_runner() -> ProcessReturn {
    use arch::interrupt::handlers::current_tick;
    use common::get_kernel_tests;
    use kernel::threading::spawn_fn;

    use crate::kernel::threading;
    let tests = unsafe { get_kernel_tests() };
    println!("running {} tests...", tests.len());
    let mut tests_failed = false;
    let max_len = tests.iter().map(|t| t.name().len()).max().unwrap_or(0);
    for test in tests {
        let dots = ".".repeat(max_len - test.name().len() + 3);
        print!("{}{} ", test.name(), dots);

        let handle = with_devices!(
            |devices| {
                if !test.config.should_panic {
                    let sink: FdEntry<StdErrTag> = DeviceBuilder::tty().serial();
                    devices.attach(sink);
                }
                for init in test.config.device_inits {
                    init(devices as *mut TaskDevices as *mut ());
                }
            },
            || { spawn_fn(test.func, args!()).expect("test spawn failed") }
        )
        .unwrap();

        let start_time = current_tick();
        match handle.wait_while(|handle| {
            let now = current_tick();
            if now - start_time >= MAX_TEST_TIME {
                arch::interrupt::without_interrupts(|| {
                    use crate::kernel::threading::tls;

                    print!("\x1b[31m[TASK TIMEOUT] \x1b[0m");
                    tls::task_data().kill(&handle.get_task().unwrap().pid(), 1);
                })
            } else {
                threading::yield_now();
            }
        }) {
            Ok(v) => {
                if v == 0 && !test.config.should_panic {
                    println!("\x1b[32m[OK]\x1b[0m");
                } else if test.config.should_panic && v != 0 {
                    println!("\x1b[33m[OK]\x1b[0m");
                } else {
                    println!("\x1b[31m[ERR]\x1b[0m");
                    tests_failed = true;
                }
            }
            Err(_) => {
                if test.config.should_panic {
                    println!("\x1b[33m[OK]\x1b[0m");
                } else {
                    println!("\x1b[1;31m[ERR]\x1b[0m");
                    tests_failed = true;
                }
            }
        };
    }
    // to allow background threads to clean up remaining resources
    threading::yield_now();
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

    use crate::kernel::threading::tls;
    eprintln!("\ntest {}", info);
    #[cfg(feature = "gkl")]
    {
        use crate::utils::locks::GKL;

        if GKL.is_locked() {
            eprintln!("GKL is locked, but the thread is killed.\nUnlocking GKL...");
            unsafe { GKL.unlock_unchecked() };
        }
    }
    tls::task_data().kill(&tls::task_data().current_pid(), 1);
    loop {
        threading::yield_now();
    }
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

#[kernel_test]
fn arg_expansion() {
    #[with_default_args]
    fn foo() {
        assert_eq!(_arg0, Arg::default());
        assert_eq!(_arg5, Arg::default());
    }

    #[with_default_args]
    fn bar(foobar: Arg) {
        assert_eq!(foobar, Arg::from_usize(42));
        assert_eq!(_arg1, Arg::default());
    }

    #[with_default_args(6)]
    fn foobar(a: usize, b: usize) {
        assert_eq!(a, b);
        assert_eq!(_arg7, Arg::default());
    }

    #[with_default_args]
    fn foobarfoo(a: usize, b: usize, c: usize, d: usize, e: usize, f: usize) {
        assert_eq!(a, f)
    }

    foo(
        Arg::default(),
        Arg::default(),
        Arg::default(),
        Arg::default(),
        Arg::default(),
        Arg::default(),
    );

    bar(
        Arg::from_usize(42),
        Arg::default(),
        Arg::default(),
        Arg::default(),
        Arg::default(),
        Arg::default(),
    );

    foobarfoo(42, 42, 42, 42, 42, 42);

    foobar(
        42,
        42,
        Arg::default(),
        Arg::default(),
        Arg::default(),
        Arg::default(),
        Arg::default(),
        Arg::default(),
    );
}

#[kernel_test(should_panic)]
fn timeout() {
    loop {
        arch::timer();
    }
    unreachable!()
}
