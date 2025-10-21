#![no_std]
#![feature(abi_x86_interrupt)]
#![feature(lazy_type_alias)]
#![feature(unsafe_cell_access)]
#![feature(stmt_expr_attributes)]
#![feature(str_from_raw_parts)]
#![allow(
    unreachable_code,
    unused_doc_comments,
    unused_variables,
    private_interfaces,
    incomplete_features,
    clippy::missing_safety_doc
)]

pub extern crate alloc;

cfg_if! {
    if #[cfg(feature = "test_run")] {
        use core::{panic::PanicInfo, time::Duration};
        use alloc::{vec::Vec, sync::Arc};

        use os_macros::with_default_args;
        use tiny_os_common::testing::TestCase;

        use crate::{
            arch::interrupt::enable_threading_interrupts,
            common::{get_kernel_tests, KernelTest},
            drivers::start_drivers,
            kernel::{
                threading::{
                    self,
                    ProcessReturn,
                    schedule::add_named_ktask,
                    spawn_fn,
                    task::{Arg, TaskRepr},
                    tls,
                    yield_now,
                },
                fd::{STDERR_FILENO, STDOUT_FILENO, FileDescriptor, File},
                fs::{self, OpenOptions, Path},
            },
        };
    } else {}
}

use cfg_if::cfg_if;
use os_macros::kernel_test;
use thiserror::Error;
pub use utils::*;

use crate::kernel::{io::IOError, threading::ThreadingError};

pub mod arch;
pub mod bootinfo;
pub mod common;
pub mod drivers;
pub mod include_bins;
pub mod kernel;
pub mod requests;
pub mod structures;
pub mod term;
mod utils;

#[cfg(feature = "test_run")]
const MAX_TEST_TIME: Duration = Duration::from_secs(10);

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
    threading::init();

    add_named_ktask(kernel_test_runner, "test runner".into());

    start_drivers();
    threading::finalize();

    enable_threading_interrupts();
    yield_now();
    unreachable!()
}

#[cfg(feature = "test_run")]
#[with_default_args]
extern "C" fn kernel_test_runner() -> ProcessReturn {
    let current = tls::task_data().get_current().unwrap();
    _ = current.add_fd(
        STDERR_FILENO,
        fs::open(Path::new("/proc/kernel/io/serial"), OpenOptions::WRITE).unwrap(),
    );
    _ = current.add_fd(
        STDOUT_FILENO,
        fs::open(Path::new("/proc/kernel/io/serial"), OpenOptions::WRITE).unwrap(),
    );
    drop(current);

    let tests: &[KernelTest] = unsafe { get_kernel_tests() };
    println!("running {} tests...", tests.len());
    let mut tests_failed = false;
    let max_len = tests.iter().map(|t| t.name().len()).max().unwrap_or(0);
    for test in tests {
        use crate::{arch::x86::current_time, kernel::threading::spawn_fn_with_init};

        let dots = ".".repeat(max_len - test.name().len() + 3);
        print!("{}{} ", test.name(), dots);

        let Ok(files): Result<Vec<(FileDescriptor, Arc<File>)>, _> =
            test.config.open_files.iter().try_fold(
                Vec::with_capacity(test.config.open_files.len()),
                |mut acc, (fd, path)| {
                    let file = fs::open(Path::new(path), OpenOptions::WRITE)?;
                    acc.push((*fd as FileDescriptor, file.into()));
                    Ok::<Vec<(FileDescriptor, Arc<File>)>, IOError>(acc)
                },
            )
        else {
            println!("\x1b[31m[ERR]\x1b[0m");
            continue;
        };

        let Ok(handle) = spawn_fn_with_init(test.func, |builder| {
            // TODO add OpenOptions to macro
            Ok(builder
                .with_args(args!())
                .with_default_files()
                .override_files(files.into_iter()))
        }) else {
            println!("\x1b[31m[ERR]\x1b[0m");
            continue;
        };

        let start_time = current_time();
        match handle.wait_while(|handle| {
            let now = current_time();
            if now - start_time >= MAX_TEST_TIME {
                arch::interrupt::without_interrupts(|| {
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
    eprintln!("\ntest {}", info);

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

pub type KernelRes<T> = Result<T, KernelError>;

#[derive(Error, Debug)]
pub enum KernelError {
    #[error("IO error:\n{0}")]
    IO(#[from] IOError),
    #[error("Threading error:\n{0}")]
    Threading(#[from] ThreadingError),
    #[error("Unknown error:\n{0}")]
    Unexpected(&'static str),
}

#[kernel_test(should_panic, silent)]
fn should_panic_err() {
    // works
    return todo!();
    assert!(true)
}

#[kernel_test(should_panic, silent)]
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
