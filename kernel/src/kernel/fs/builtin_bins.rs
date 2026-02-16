// builtin utilities, which must currently live in the kernel (like shutdown).
// they may be executed via execve(path).
// execve may treat them differntly from 'real' binaries though.

use alloc::{boxed::Box, str, vec::Vec};

use os_macros::with_default_args;
use tinyos_abi::{
    consts::STDIN_FILENO,
    flags::{OpenOptions, UnlinkOptions},
    types::FileDescriptor,
};

use crate::{
    drivers::wait_manager::{add_queue, remove_queue, wait_self},
    eprintln,
    exit_qemu,
    kernel::{
        fd::FileRepr,
        fs::{self, Path, PathBuf},
        init::INCLUDED_BINS,
        io::Write,
        threading::{
            schedule::current_task,
            task::{Arg, TaskRepr},
            tls,
            wait::{
                QueueHandle,
                queues::{GenericWaitQueue, WaitQueue},
            },
        },
    },
    println,
    serial_println,
};

pub const BUILTIN_MARKER: &[u8] = b"tiny_builtin";

pub trait Executable {
    const PATH: &str;
    fn execute(argv: Option<Box<[u8]>>, envp: Option<Box<[u8]>>) -> usize;
    fn init();
}

pub fn init() {
    ShutDown::init();
    Serial::init();
    ReadFromFD::init();
}

#[with_default_args]
pub extern "C" fn execute(path: Arg, argc: Arg, argv: Arg, envc: Arg, envp: Arg) -> usize {
    let path = unsafe { path.as_val::<PathBuf>() };
    let argv = unsafe { argv.as_val::<Option<Box<[u8]>>>() };
    let envp = unsafe { envp.as_val::<Option<Box<[u8]>>>() };
    match path.as_str() {
        ShutDown::PATH => ShutDown::execute(argv, envp),
        Serial::PATH => Serial::execute(argv, envp),
        ReadFromFD::PATH => ReadFromFD::execute(argv, envp),
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

    fn execute(argv: Option<Box<[u8]>>, envp: Option<Box<[u8]>>) -> usize {
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

    fn execute(argv: Option<Box<[u8]>>, envp: Option<Box<[u8]>>) -> usize {
        if argv.is_none() {
            return 0;
        }
        let str = unsafe { str::from_boxed_utf8_unchecked(argv.unwrap()) };
        serial_println!("received argv: {}", str);
        0
    }
}

pub struct ReadFromFD;

impl Executable for ReadFromFD {
    const PATH: &str = "/ram/bin/read_from";

    fn init() {
        init_fake_bin(Path::new(Self::PATH));
    }

    fn execute(argv: Option<Box<[u8]>>, envp: Option<Box<[u8]>>) -> usize {
        let fd = if argv.is_none() {
            STDIN_FILENO
        } else {
            let str = unsafe { str::from_boxed_utf8_unchecked(argv.unwrap()) };
            let num = str.parse::<FileDescriptor>().unwrap_or(STDIN_FILENO);
            num
        };

        serial_println!("read from {}", fd);
        serial_println!(
            "file at fd: {:?}",
            tls::task_data()
                .current_thread()
                .unwrap()
                .core
                .fd_table
                .read()
                .get(&fd)
        );

        let mut buf = Vec::new();
        let mut temp_buf = [0; 64];

        let f = tls::task_data().current_thread().unwrap().fd(fd).unwrap();
        let waiter = f.get_waiter();
        if let Some(waiter) = &waiter {
            add_queue(
                QueueHandle::from_owned(Box::new(GenericWaitQueue::new()) as Box<dyn WaitQueue>),
                waiter.q_type.clone(),
            );
        }
        while let Ok(n) = f.read_continuous(&mut temp_buf)
            && n >= 0
        {
            if n == 0
                && let Some(cond) = &waiter
            {
                let condition = [cond.clone()];
                wait_self(&condition).unwrap();
                continue;
            }
            let old_len = buf.len();
            buf.resize(old_len + n as usize, Default::default());
            buf[old_len..].swap_with_slice(&mut temp_buf[..n as usize]);
        }

        let contents = str::from_utf8(&buf).unwrap();
        if let Some(waiter) = waiter {
            remove_queue(&waiter.q_type);
        }
        println!("read \n{}\n from fd {}", contents, fd);
        serial_println!("read \n{}\n from fd {}", contents, fd);
        0
    }
}
