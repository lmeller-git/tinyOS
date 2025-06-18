use super::TTYSink;
use crate::{
    arch,
    kernel::devices::{FdEntryType, RawFdEntry, with_current_device_list},
    locks::thread_safe::Mutex,
    print, serial_print,
    term::_print,
};
use alloc::{collections::vec_deque::VecDeque, sync::Arc, vec::Vec};
use conquer_once::spin::OnceCell;
use crossbeam::queue::SegQueue;
use x86_64::instructions::interrupts::without_interrupts;

pub static SERIALBACKEND: OnceCell<Arc<SerialBackend>> = OnceCell::uninit();
pub static FBBACKEND: OnceCell<Arc<FbBackend>> = OnceCell::uninit();

pub fn init_tty_sinks() {
    _ = SERIALBACKEND.try_init_once(SerialBackend::new);
    _ = FBBACKEND.try_init_once(FbBackend::new);
}

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
        // self.buffer.lock().extend(bytes.iter());
        for byte in bytes {
            self.buffer.push(*byte);
        }
    }

    fn flush(&self) {
        // let all_bytes = self.buffer.lock().drain(..).collect::<Vec<u8>>();
        let mut all_bytes = Vec::new();
        while let Some(byte) = self.buffer.pop() {
            all_bytes.push(byte);
        }
        let out = str::from_utf8(&all_bytes).unwrap();
        arch::_serial_print(format_args!("{}", out));
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FbBackend {
    buffer: Mutex<VecDeque<u8>>,
}

impl FbBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffer: Mutex::new(VecDeque::new()),
        })
    }
}

impl TTYSink for FbBackend {
    fn write(&self, bytes: &[u8]) {
        self.buffer.lock().extend(bytes.iter());
    }

    fn flush(&self) {
        let all_bytes = self.buffer.lock().drain(..).collect::<Vec<u8>>();
        let out = str::from_utf8(&all_bytes).unwrap();
        _print(format_args!("{}", out));
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TTYReceiver<T: TTYSink> {
    backend: Arc<T>,
}

impl<T: TTYSink> TTYReceiver<T> {
    pub fn new(backend: Arc<T>) -> Self {
        Self { backend }
    }
}

impl<T: TTYSink> TTYSink for TTYReceiver<T> {
    fn write(&self, bytes: &[u8]) {
        self.backend.write(bytes);
    }

    fn flush(&self) {
        self.backend.flush();
    }
}
