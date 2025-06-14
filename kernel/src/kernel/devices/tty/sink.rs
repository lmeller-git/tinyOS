use super::TTYSink;
use crate::{
    kernel::devices::{FdEntryType, RawFdEntry, with_current_device_list},
    locks::thread_safe::Mutex,
    print, serial_print,
    term::_print,
};
use alloc::{collections::vec_deque::VecDeque, sync::Arc, vec::Vec};
use conquer_once::spin::OnceCell;

pub static SERIALBACKEND: OnceCell<Arc<SerialBackend>> = OnceCell::uninit();
pub static FBBACKEND: OnceCell<Arc<FbBackend>> = OnceCell::uninit();

pub fn init_tty_sinks() {
    SERIALBACKEND.init_once(SerialBackend::new);
    FBBACKEND.init_once(FbBackend::new);
}

#[derive(Debug, PartialEq, Eq)]
pub struct SerialBackend {
    buffer: Mutex<VecDeque<u8>>,
}

impl SerialBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffer: Mutex::new(VecDeque::new()),
        })
    }
}

impl TTYSink for SerialBackend {
    fn write(&self, bytes: &[u8]) {
        self.buffer.lock().extend(bytes.iter());
    }

    fn flush(&self) {
        let all_bytes = self.buffer.lock().drain(..).collect::<Vec<u8>>();
        let out = str::from_utf8(&all_bytes).unwrap();
        serial_print!("{}", out);
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

//TODO write a macro for these (and others)
pub fn __write_stdout(input: &str) {
    let bytes = input.as_bytes();
    with_current_device_list(|devices| {
        if let Some(devices) = devices.get(FdEntryType::StdOut) {
            let RawFdEntry::TTYSink(sinks) = devices else {
                unreachable!()
            };
            for s in sinks {
                s.write(bytes);
            }
        }
    });
}

pub fn __write_stderr(input: &str) {
    let bytes = input.as_bytes();
    with_current_device_list(|devices| {
        if let Some(devices) = devices.get(FdEntryType::StdErr) {
            let RawFdEntry::TTYSink(sinks) = devices else {
                unreachable!()
            };
            for s in sinks {
                s.write(bytes);
            }
        }
    });
}

pub fn __write_debug(input: &str) {
    let bytes = input.as_bytes();
    with_current_device_list(|devices| {
        if let Some(devices) = devices.get(FdEntryType::DebugSink) {
            let RawFdEntry::TTYSink(sinks) = devices else {
                unreachable!()
            };
            for s in sinks {
                s.write(bytes);
            }
        }
    });
}
