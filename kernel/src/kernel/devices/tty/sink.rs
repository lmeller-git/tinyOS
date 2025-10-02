use alloc::sync::Arc;

use conquer_once::spin::OnceCell;
use crossbeam::queue::SegQueue;

use super::TTYSink;
use crate::{arch, create_device_file, kernel::devices::tty::TTYSource, term::_print};

pub static SERIALBACKEND: OnceCell<Arc<SerialBackend>> = OnceCell::uninit();
pub static FBBACKEND: OnceCell<Arc<FbBackend>> = OnceCell::uninit();

pub const SERIAL_FILE: &str = "/kernel/io/serial";
pub const FBBACKEND_FILE: &str = "/kernel/io/fbbackend";

pub fn init_tty_sinks() {
    _ = SERIALBACKEND.try_init_once(SerialBackend::new);
    _ = FBBACKEND.try_init_once(FbBackend::new);

    let r = create_device_file!(SERIALBACKEND.get().unwrap().clone(), SERIAL_FILE);
    let r = create_device_file!(FBBACKEND.get().unwrap().clone(), FBBACKEND_FILE);
}

// the read_locks are only necessary if multiple instances of these Backends are alive at once, as ChunkedArrayQueue is mpsc. Currently this is not the case.
#[derive(Debug)]
pub struct SerialBackend {
    buffer: SegQueue<u8>,
}

impl SerialBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffer: SegQueue::new(),
        })
    }
}

impl TTYSink for SerialBackend {
    fn write(&self, bytes: &[u8]) {
        for byte in bytes {
            self.buffer.push(*byte);
        }
    }

    fn flush(&self) {
        while let Some(byte) = self.buffer.pop() {
            arch::_raw_serial_print(&[byte]);
        }
    }
}

impl TTYSource for SerialBackend {
    fn read(&self) -> Option<u8> {
        None
    }
}

#[derive(Debug)]
pub struct FbBackend {
    buffer: SegQueue<u8>,
}

impl FbBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffer: SegQueue::new(),
        })
    }
}

impl TTYSink for FbBackend {
    fn write(&self, bytes: &[u8]) {
        for byte in bytes {
            self.buffer.push(*byte);
        }
    }

    fn flush(&self) {
        while let Some(byte) = self.buffer.pop() {
            _print(format_args!("{}", char::from_u32(byte as u32).unwrap()));
        }
    }
}

impl TTYSource for FbBackend {
    fn read(&self) -> Option<u8> {
        None
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TTYReceiver<T: TTYSink + TTYSource> {
    backend: Arc<T>,
}

impl<T: TTYSink + TTYSource> TTYReceiver<T> {
    pub fn new(backend: Arc<T>) -> Self {
        Self { backend }
    }
}

impl<T: TTYSink + TTYSource> TTYSink for TTYReceiver<T> {
    fn write(&self, bytes: &[u8]) {
        self.backend.write(bytes);
    }

    fn flush(&self) {
        self.backend.flush();
    }
}

impl<T: TTYSource + TTYSink> TTYSource for TTYReceiver<T> {
    fn read(&self) -> Option<u8> {
        self.backend.read()
    }
}
