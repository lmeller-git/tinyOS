// builtin utilities, which must currently live in the kernel (like shutdown).
// they may be executed via execve(path).
// execve may treat them differntly from 'real' binaries though.

use os_macros::with_default_args;
use tinyos_abi::flags::{OpenOptions, UnlinkOptions};

use crate::{
    eprintln,
    exit_qemu,
    kernel::{
        fs::{self, Path},
        init::INCLUDED_BINS,
        io::Write,
        threading::task::Arg,
    },
    println,
};

pub const BUILTIN_MARKER: &[u8] = b"tiny_builtin";

pub trait Executable {
    const PATH: &str;
    fn execute() -> usize;
}

pub fn init() {
    ShutDown::init();
}

#[with_default_args]
pub extern "C" fn execute(path: Arg) -> usize {
    let path = unsafe { path.as_val::<&Path>() };
    match path.as_str() {
        ShutDown::PATH => ShutDown::execute(),
        _ => 0,
    }
}

pub struct ShutDown;

impl ShutDown {
    fn init() {
        let mut bin_path = Path::new(INCLUDED_BINS).to_owned();
        bin_path.push("shutdown");
        if let Ok(file) = fs::open(&bin_path, OpenOptions::CREATE | OpenOptions::WRITE) {
            if let Err(e) = file.write_all(BUILTIN_MARKER, 0) {
                eprintln!(
                    "could not write data into builtin binary shutdown. It will not be available"
                );
                fs::rm(&bin_path, UnlinkOptions::empty()).unwrap();
            }
        } else {
            eprintln!("failed to initialize shutdown binary");
        }
    }
}

impl Executable for ShutDown {
    const PATH: &str = "/ram/bin/shutdown";

    fn execute() -> usize {
        println!("shutting down system...");
        exit_qemu(crate::QemuExitCode::Success);
        unreachable!()
    }
}
