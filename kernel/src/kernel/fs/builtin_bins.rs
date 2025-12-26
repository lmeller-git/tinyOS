// builtin utilities, which must currently live in the kernel (like shutdown).
// they may be executed via execve(path).
// execve may treat them differntly from 'real' binaries though.

use alloc::str;

use os_macros::with_default_args;
use tinyos_abi::{
    consts::STDIN_FILENO,
    flags::{OpenOptions, UnlinkOptions},
    types::FileDescriptor,
};

use crate::{
    eprintln,
    exit_qemu,
    kernel::{
        fs::{self, Path},
        init::INCLUDED_BINS,
        io::Write,
        threading::{
            schedule::current_task,
            task::{Arg, TaskRepr},
        },
    },
    println,
    serial_println,
};

pub const BUILTIN_MARKER: &[u8] = b"tiny_builtin";

pub trait Executable {
    const PATH: &str;
    fn execute(argc: *const u8, argv: *const u8, argc_size: usize, argv_size: usize) -> usize;
    fn init();
}

pub fn init() {
    ShutDown::init();
    Serial::init();
    ReadFromFD::init();
}

#[with_default_args]
pub extern "C" fn execute(
    path: Arg,
    argc: Arg,
    argv: Arg,
    argc_size: Arg,
    argv_size: Arg,
) -> usize {
    let path = unsafe { path.as_val::<&Path>() };
    let argc = unsafe { argc.as_val::<*const u8>() };
    let argv = unsafe { argv.as_val::<*const u8>() };
    let argc_size = unsafe { argc_size.as_val::<usize>() };
    let argv_size = unsafe { argv_size.as_val::<usize>() };
    match path.as_str() {
        ShutDown::PATH => ShutDown::execute(argc, argv, argc_size, argv_size),
        Serial::PATH => Serial::execute(argc, argv, argc_size, argv_size),
        _ => 0,
    }
}

fn init_fake_bin(path: &Path) {
    if let Ok(file) = fs::open(path, OpenOptions::CREATE | OpenOptions::WRITE) {
        if let Err(e) = file.write_all(BUILTIN_MARKER, 0) {
            eprintln!(
                "could not write data into builtin binary {}. It will not be available",
                path
            );
            fs::rm(path, UnlinkOptions::empty()).unwrap();
        }
    } else {
        eprintln!("failed to initialize {} binary", path);
    }
}

pub struct ShutDown;

impl Executable for ShutDown {
    const PATH: &str = "/ram/bin/shutdown";

    fn init() {
        init_fake_bin(Path::new(Self::PATH));
    }

    fn execute(argc: *const u8, argv: *const u8, argc_size: usize, argv_size: usize) -> usize {
        println!("shutting down system...");
        exit_qemu(crate::QemuExitCode::Success);
        unreachable!()
    }
}

pub struct Serial;

impl Executable for Serial {
    const PATH: &str = "/ram/bin/serial";

    fn init() {
        init_fake_bin(Path::new(Self::PATH));
    }

    fn execute(argc: *const u8, argv: *const u8, argc_size: usize, argv_size: usize) -> usize {
        if argc.is_null() {
            return 0;
        }
        let str = unsafe { str::from_raw_parts(argc, argc_size) };
        serial_println!("received argc: {}", str);
        0
    }
}

pub struct ReadFromFD;

impl Executable for ReadFromFD {
    const PATH: &str = "/ram/bin/read_from";

    fn init() {
        init_fake_bin(Path::new(Self::PATH));
    }

    fn execute(argc: *const u8, argv: *const u8, argc_size: usize, argv_size: usize) -> usize {
        let fd = if argc.is_null() || argc_size < size_of::<FileDescriptor>() {
            STDIN_FILENO
        } else {
            *unsafe { &*(argc as *const FileDescriptor) }
        };

        let contents = current_task()
            .unwrap()
            .fd(fd)
            .unwrap()
            .read_all_as_str()
            .unwrap();
        println!("read \n{}\n from fd {}", contents, fd);
        serial_println!("read \n{}\n from fd {}", contents, fd);
        0
    }
}
