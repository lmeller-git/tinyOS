use super::TTYSink;
use crate::{
    arch,
    data_structures::ChunkedArrayQueue,
    kernel::devices::{FdEntryType, RawFdEntry, with_current_device_list},
    locks::thread_safe::Mutex,
    print, serial_print,
    term::_print,
};
use alloc::{collections::vec_deque::VecDeque, sync::Arc, vec::Vec};
use conquer_once::spin::OnceCell;
use crossbeam::queue::SegQueue;

pub static SERIALBACKEND: OnceCell<Arc<SerialBackend>> = OnceCell::uninit();
pub static FBBACKEND: OnceCell<Arc<FbBackend>> = OnceCell::uninit();

pub fn init_tty_sinks() {
    _ = SERIALBACKEND.try_init_once(SerialBackend::new);
    _ = FBBACKEND.try_init_once(FbBackend::new);
}

#[derive(Debug)]
pub struct SerialBackend {
    buffer: ChunkedArrayQueue<50, u8>,
    read_lock: Mutex<()>,
}

impl SerialBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffer: ChunkedArrayQueue::new(),
            read_lock: Mutex::new(()),
        })
    }
}

impl TTYSink for SerialBackend {
    fn write(&self, bytes: &[u8]) {
        // here we might already want to split bytes into chinks of length N in order to prevent locking -> this would however allow interleaving outputs
        self.buffer.push(bytes);
    }

    fn flush(&self) {
        let mut buf = [0; 50];
        let lock = self.read_lock.lock();
        let Ok(n) = self.buffer.read(&mut buf) else {
            //TODO: handle
            panic!("cannot handle buf read err")
        };
        drop(lock);
        arch::_raw_serial_print(&buf[..n]);
    }
}

#[derive(Debug)]
pub struct FbBackend {
    buffer: ChunkedArrayQueue<50, u8>,
    read_lock: Mutex<()>,
}

impl FbBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffer: ChunkedArrayQueue::new(),
            read_lock: Mutex::new(()),
        })
    }
}

impl TTYSink for FbBackend {
    fn write(&self, bytes: &[u8]) {
        // here we might already want to split bytes into chinks of length N in order to prevent locking -> this would however allow interleaving outputs
        self.buffer.push(bytes);
    }

    fn flush(&self) {
        let mut buf = [0; 50];
        let lock = self.read_lock.lock();
        let Ok(n) = self.buffer.read(&mut buf) else {
            //TODO: handle
            panic!("Cannot handle buf read err")
        };
        drop(lock);
        if let Ok(out) = str::from_utf8(&buf[..n]) {
            _print(format_args!("{}", out));
        }
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
