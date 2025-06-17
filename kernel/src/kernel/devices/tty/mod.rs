use core::fmt::Debug;

use alloc::{sync::Arc, vec};
use sink::{FBBACKEND, FbBackend, SERIALBACKEND, SerialBackend, TTYReceiver};
use source::{KEYBOARDBACKEND, KeyboardBackend, TTYInput};

use super::{FdEntry, FdTag, RawFdEntry};

pub mod io;
pub mod sink;
pub mod source;

pub trait TTYSink: Debug {
    fn write(&self, bytes: &[u8]);
    fn flush(&self);
}

pub trait TTYSource: Debug {
    fn read(&self) -> Option<u8>;
}

pub struct TTYBuilder {}

impl TTYBuilder {
    pub fn keyboard<T: FdTag>(self) -> FdEntry<T> {
        let backend = TTYInput::new(KEYBOARDBACKEND.get().unwrap().clone());
        FdEntry::new(RawFdEntry::TTYSource(vec![Arc::new(backend)]))
    }

    pub fn serial<T: FdTag>(self) -> FdEntry<T> {
        let backend = TTYReceiver::new(SERIALBACKEND.get().unwrap().clone());
        FdEntry::new(RawFdEntry::TTYSink(vec![Arc::new(backend)]))
    }

    pub fn fb<T: FdTag>(self) -> FdEntry<T> {
        let backend = TTYReceiver::new(FBBACKEND.get().unwrap().clone());
        FdEntry::new(RawFdEntry::TTYSink(vec![Arc::new(backend)]))
    }
}

pub fn init() {
    sink::init_tty_sinks();
    source::init_source_tty();
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
        todo!()
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
