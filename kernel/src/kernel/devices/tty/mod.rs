use alloc::{collections::vec_deque::VecDeque, sync::Arc};
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
        threading::wait::{QueuTypeCondition, QueueType},
    },
    sync::{get_next_lock_var, locks::Mutex},
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

#[derive(Debug)]
pub struct Pipe {
    buf: Mutex<VecDeque<u8>>,
    lock_descriptor: u64,
}

impl Pipe {
    pub fn new() -> Self {
        let lock_descriptor = get_next_lock_var();
        Self {
            buf: Mutex::default(),
            lock_descriptor,
        }
    }
}

impl Write for Pipe {
    fn write(&self, buf: &[u8], _offset: usize) -> IOResult<usize> {
        self.buf.lock().extend(buf);
        Ok(buf.len())
    }
}

impl Read for Pipe {
    fn read(&self, buf: &mut [u8], _offset: usize) -> IOResult<usize> {
        let mut internal = self.buf.lock();
        let len = buf.len().min(internal.len());
        buf[..len]
            .iter_mut()
            .zip(internal.drain(..len))
            .for_each(|(buf_, item)| *buf_ = item);
        Ok(len)
    }
}

impl IOCapable for Pipe {}

impl FileRepr for Pipe {
    fn node_type(&self) -> NodeType {
        NodeType::Void
    }

    fn get_waiter(&self) -> Option<QueuTypeCondition> {
        Some(QueuTypeCondition::new(QueueType::Lock(
            self.lock_descriptor,
        )))
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
