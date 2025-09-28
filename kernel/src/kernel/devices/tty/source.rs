use alloc::sync::Arc;

use conquer_once::spin::OnceCell;

use super::TTYSource;
use crate::{
    drivers::keyboard::get_current_next,
    kernel::devices::tty::TTYSink,
    register_device_file,
};

pub static KEYBOARDBACKEND: OnceCell<Arc<KeyboardBackend>> = OnceCell::uninit();

pub fn init_source_tty() {
    KEYBOARDBACKEND.init_once(KeyboardBackend::new);
    register_device_file!(KEYBOARDBACKEND.get().unwrap().clone(), "/keyboard");
}

#[derive(Debug, PartialEq, Eq)]
pub struct KeyboardBackend;

impl KeyboardBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl TTYSource for KeyboardBackend {
    fn read(&self) -> Option<u8> {
        get_current_next().ok()
    }
}

impl TTYSink for KeyboardBackend {
    fn write(&self, bytes: &[u8]) {}

    fn flush(&self) {}
}

#[derive(Debug, PartialEq, Eq)]
pub struct TTYInput<T: TTYSource + TTYSink> {
    backend: Arc<T>,
}

impl<T: TTYSource + TTYSink> TTYInput<T> {
    pub fn new(backend: Arc<T>) -> Self {
        Self { backend }
    }
}

impl<T: TTYSource + TTYSink> TTYSource for TTYInput<T> {
    fn read(&self) -> Option<u8> {
        self.backend.read()
    }
}

impl<T: TTYSink + TTYSource> TTYSink for TTYInput<T> {
    fn write(&self, bytes: &[u8]) {
        self.backend.write(bytes);
    }

    fn flush(&self) {
        self.backend.flush();
    }
}
