use alloc::sync::Arc;
use core::fmt::Debug;

use hashbrown::HashMap;

use crate::{
    impl_empty_read,
    impl_empty_write,
    impl_file_for_wr,
    kernel::{
        devices::Null,
        fd::{FileRepr, IOCapable},
        fs::NodeType,
        io::{IOError, IOResult, Read, Write},
    },
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
            Ok(1)
        } else {
            Ok(0)
        }
    }
}

#[macro_export]
macro_rules! impl_write_for_tty {
    (@impl [$($impl_generics:tt)*] $name:ty) => {
        impl<$($impl_generics)*> $crate::kernel::io::Write for $name {
            fn write(&self, buf: &[u8], offset: usize) -> $crate::kernel::io::IOResult<usize> {
                $crate::kernel::devices::tty::TTYSink::write(self, buf);
                Ok(buf.len())}

        }
    };

    ($name:ty) => {
        impl_write_for_tty!(@impl [] $name);
    };

    ($name:ty where [$($generics:tt)*]) => {
        impl_write_for_tty!(@impl [$($generics)*] $name);
    };
}

#[macro_export]
macro_rules! impl_read_for_tty {
    (@impl [$($impl_generics:tt)*] $name:ty) => {
        impl<$($impl_generics)*> $crate::kernel::io::Read for $name {
            fn read(&self, buf: &mut [u8], offset: usize) -> $crate::kernel::io::IOResult<usize> {
                $crate::kernel::devices::tty::TTYSource::read_buf(self, buf, offset)
            }
        }
    };

    ($name:ty) => {
        impl_read_for_tty!(@impl [] $name);
    };

    ($name:ty where [$($generics:tt)*]) => {
        impl_read_for_tty!(@impl [$($generics)*] $name);
    };
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

impl_empty_read!(Null);
impl_empty_write!(Null);
impl_file_for_wr!(Null: NodeType::Void);

#[macro_export]
macro_rules! print {
    () => {};
    ($($arg:tt)*) => { $crate::kernel::devices::tty::io::__write_stdout(format_args!("{}", format_args!($($arg)*))) };
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
        $crate::kernel::devices::tty::io::__write_stderr(format_args!("\x1b[31m[KERR]\x1b[0m {}", format_args!($($arg)*)))
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
        $crate::kernel::devices::tty::io::__serial_stub(format_args!("\x1b[34m[KINFO]\x1b[0m {}", format_args!($($arg)*)))
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
