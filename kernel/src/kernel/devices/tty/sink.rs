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
    #[cfg(feature = "custom_ds")]
    buffer: ChunkedArrayQueue<100, u8>,
    #[cfg(not(feature = "custom_ds"))]
    buffer: SegQueue<u8>,
    read_lock: Mutex<()>,
}

impl SerialBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            #[cfg(feature = "custom_ds")]
            buffer: ChunkedArrayQueue::new(),
            #[cfg(not(feature = "custom_ds"))]
            buffer: SegQueue::new(),
            read_lock: Mutex::new(()),
        })
    }
}

impl TTYSink for SerialBackend {
    fn write(&self, bytes: &[u8]) {
        #[cfg(feature = "custom_ds")]
        {
            // here we might already want to split bytes into chunks of length N in order to prevent locking -> this would however allow interleaving outputs
            // however a locked push with gkl enabled might lead to a deadlock, as we cannot acquire the lock to flush anymore
            // this is currently not necessary, as ChunkedArrayQueue uses spin::Mutex, which does not lock the gkl

            // #[cfg(feature = "gkl")]
            // for chunk in bytes.chunks(self.buffer.len()) {
            //     self.buffer.push(chunk);
            // }
            // #[cfg(not(feature = "gkl"))]
            self.buffer.push(bytes);
        }
        #[cfg(not(feature = "custom_ds"))]
        for byte in bytes {
            self.buffer.push(*byte);
        }
    }

    fn flush(&self) {
        #[cfg(not(feature = "custom_ds"))]
        while let Some(byte) = self.buffer.pop() {
            arch::_raw_serial_print(&[byte]);
        }
        #[cfg(feature = "custom_ds")]
        {
            let mut buf = [0; 50];
            // need to acquire the lock if multiple consumers exist. Currently they do not

            // let lock = self.read_lock.lock();
            let Ok(n) = self.buffer.read(&mut buf) else {
                //TODO: handle
                panic!("cannot handle buf read err")
            };
            // drop(lock);
            arch::_raw_serial_print(&buf[..n]);
        }
    }
}

#[derive(Debug)]
pub struct FbBackend {
    #[cfg(feature = "custom_ds")]
    buffer: ChunkedArrayQueue<100, u8>,
    #[cfg(not(feature = "custom_ds"))]
    buffer: SegQueue<u8>,
    read_lock: Mutex<()>,
}

impl FbBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            #[cfg(feature = "custom_ds")]
            buffer: ChunkedArrayQueue::new(),
            #[cfg(not(feature = "custom_ds"))]
            buffer: SegQueue::new(),
            read_lock: Mutex::new(()),
        })
    }
}

impl TTYSink for FbBackend {
    fn write(&self, bytes: &[u8]) {
        #[cfg(feature = "custom_ds")]
        {
            // here we might already want to split bytes into chinks of length N in order to prevent locking -> this would however allow interleaving outputs

            // #[cfg(feature = "gkl")]
            // for chunk in bytes.chunks(self.buffer.len()) {
            //     self.buffer.push(chunk);
            // }
            // #[cfg(not(feature = "gkl"))]
            self.buffer.push(bytes);
        }
        #[cfg(not(feature = "custom_ds"))]
        for byte in bytes {
            self.buffer.push(*byte);
        }
    }

    fn flush(&self) {
        #[cfg(feature = "custom_ds")]
        {
            let mut buf = [0; 50];
            // let lock = self.read_lock.lock();
            let Ok(n) = self.buffer.read(&mut buf) else {
                //TODO: handle
                panic!("Cannot handle buf read err")
            };
            // drop(lock);
            if let Ok(out) = str::from_utf8(&buf[..n]) {
                _print(format_args!("{}", out));
            }
        }
        #[cfg(not(feature = "custom_ds"))]
        while let Some(byte) = self.buffer.pop() {
            _print(format_args!("{}", char::from_u32(byte as u32).unwrap()));
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
