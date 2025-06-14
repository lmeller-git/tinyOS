use alloc::sync::Arc;
use conquer_once::spin::OnceCell;

use crate::drivers::keyboard::get_current_next;

use super::TTYSource;

pub static KEYBOARDBACKEND: OnceCell<Arc<KeyboardBackend>> = OnceCell::uninit();

pub fn init_source_tty() {
    KEYBOARDBACKEND.init_once(KeyboardBackend::new);
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

#[derive(Debug, PartialEq, Eq)]
pub struct TTYInput<T: TTYSource> {
    backend: Arc<T>,
}

impl<T: TTYSource> TTYInput<T> {
    pub fn new(backend: Arc<T>) -> Self {
        Self { backend }
    }
}

impl<T: TTYSource> TTYSource for TTYInput<T> {
    fn read(&self) -> Option<u8> {
        self.backend.read()
    }
}
