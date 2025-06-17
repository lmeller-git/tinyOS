use core::fmt::Debug;

use alloc::{sync::Arc, vec};
use sink::{FBBACKEND, FbBackend, SERIALBACKEND, SerialBackend, TTYReceiver};
use source::{KEYBOARDBACKEND, KeyboardBackend, TTYInput};

use super::{FdEntry, FdTag, RawFdEntry};

mod io;
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

// #[macro_export]
macro_rules! print {
    () => {};
    ($($arg:tt)*) => { $crate::kernel::devices::tty::sink::__write_stdout(format_args!($($arg)*).as_str().unwrap()) };
}

// #[macro_export]
macro_rules! println {
    () => { $crate::print!("\n")};
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*))};
}

macro_rules! dbg {
    () => {};
    ($($arg:tt)*) => {
        todo!()
    };
}

macro_rules! eprint {
    () => {};
    ($($arg:tt)*) => {
        todo!()
    };
}

macro_rules! eprintln {
    () => {
        $crate::eprint!("\n")
    };
    ($($arg:tt)*) => {
        todo!()
    };
}
