use alloc::sync::Arc;
use core::fmt::Debug;

use hashbrown::HashMap;
use sink::{FBBACKEND, SERIALBACKEND, TTYReceiver};
use source::{KEYBOARDBACKEND, TTYInput};

use super::{FdEntry, FdTag, RawDeviceID, RawFdEntry};
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

pub struct TTYBuilder {
    id: RawDeviceID,
}

impl TTYBuilder {
    pub(super) fn new(id: RawDeviceID) -> Self {
        Self { id }
    }

    pub fn keyboard<T: FdTag>(self) -> FdEntry<T> {
        let backend = TTYInput::new(KEYBOARDBACKEND.get().unwrap().clone());
        FdEntry::new(RawFdEntry::TTYSource(self.id, Arc::new(backend)), self.id)
    }

    pub fn serial<T: FdTag>(self) -> FdEntry<T> {
        let backend = TTYReceiver::new(SERIALBACKEND.get().unwrap().clone());
        let mut new_map = HashMap::new();
        new_map.insert(self.id, Arc::new(backend) as Arc<dyn TTYSink>);
        FdEntry::new(RawFdEntry::TTYSink(new_map), self.id)
    }

    pub fn fb<T: FdTag>(self) -> FdEntry<T> {
        let backend = TTYReceiver::new(FBBACKEND.get().unwrap().clone());
        let mut new_map = HashMap::new();
        new_map.insert(self.id, Arc::new(backend) as Arc<dyn TTYSink>);
        FdEntry::new(RawFdEntry::TTYSink(new_map), self.id)
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
