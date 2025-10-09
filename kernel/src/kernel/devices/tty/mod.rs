use alloc::sync::Arc;
use core::fmt::Debug;

use hashbrown::HashMap;
use sink::{FBBACKEND, SERIALBACKEND, TTYReceiver};
use source::{KEYBOARDBACKEND, TTYInput};

use crate::kernel::{
    devices::Null,
    fd::{FileRepr, IOCapable},
    io::{IOError, IOResult, Read, Write},
};

pub mod io;
pub mod sink;
pub mod source;

pub fn init() {
    sink::init_tty_sinks();
    source::init_source_tty();
}

pub trait TTYSink: Debug + Send + Sync {
    fn write(&self, bytes: &[u8]);
    fn flush(&self);
}

pub trait TTYSource: Debug + Send + Sync {
    fn read(&self) -> Option<u8>;
    fn read_buf(&self, buf: &mut [u8], offset: usize) -> IOResult<usize> {
        if let Some(r) = self.read() {
            *buf.get_mut(0).ok_or(IOError::simple(
                crate::kernel::fs::FSErrorKind::UnexpectedEOF,
            ))? = r;
            Ok(0)
        } else {
            Err(IOError::simple(
                crate::kernel::fs::FSErrorKind::UnexpectedEOF,
            ))
        }
    }
}

impl<T: TTYSink + TTYSource> FileRepr for T {
    fn fstat(&self) -> crate::kernel::fd::FStat {
        crate::kernel::fd::FStat::new()
    }

    fn node_type(&self) -> crate::kernel::fs::NodeType {
        crate::kernel::fs::NodeType::File
    }
}

impl<T: TTYSink + TTYSource> IOCapable for T {}

impl<T: TTYSink + TTYSource> Read for T {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        TTYSource::read_buf(self, buf, offset)
    }
}

impl<T: TTYSink + TTYSource> Write for T {
    fn write(&self, buf: &[u8], offset: usize) -> IOResult<usize> {
        TTYSink::write(self, buf);
        Ok(buf.len())
    }
}

impl TTYSink for Null {
    fn write(&self, bytes: &[u8]) {}

    fn flush(&self) {}
}

impl TTYSource for Null {
    fn read(&self) -> Option<u8> {
        None
    }
}

#[macro_export]
macro_rules! print {
    () => {};
    ($($arg:tt)*) => { $crate::kernel::devices::tty::io::__write_stdout(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n")};
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*))};
}

#[macro_export]
macro_rules! dbg {
    () => {};
    ($($arg:tt)*) => {
        todo!()
    };
}

#[macro_export]
macro_rules! eprint {
    () => {};
    ($($arg:tt)*) => {
        $crate::kernel::devices::tty::io::__write_stderr(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! eprintln {
    () => {
        $crate::eprint!("\n")
    };
    ($($arg:tt)*) => {
        $crate::eprint!("{}\n", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::serial_print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::serial_print!("{}\n", format_args!($($arg)*))
    };
}
#[macro_export]
macro_rules! serial_print {
    () => {};
    ($($arg:tt)*) => {
        $crate::kernel::devices::tty::io::__serial_stub(format_args!($($arg)*))
    };
}
#[macro_export]
macro_rules! cross_println {
    () => {
        $crate::cross_print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::cross_print!("{}\n", format_args!($($arg)*))
    };
}
#[macro_export]
macro_rules! cross_print {
    () => {};
    ($($arg:tt)*) => {
        $crate::print!("{}", format_args!($($arg)*));
        $crate::serial_print!("{}", format_args!($($arg)*))
    };
}
