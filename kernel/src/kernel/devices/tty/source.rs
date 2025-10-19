use alloc::sync::Arc;

use conquer_once::spin::OnceCell;

use super::TTYSource;
use crate::{
    drivers::keyboard::get_current_next,
    impl_empty_write,
    impl_file_for_wr,
    impl_read_for_tty,
    kernel::{devices::tty::TTYSink, fs::NodeType},
    register_device_file,
};

pub static KEYBOARDBACKEND: OnceCell<Arc<KeyboardBackend>> = OnceCell::uninit();

pub fn init_source_tty() {
    KEYBOARDBACKEND.init_once(KeyboardBackend::new);
    register_device_file!(
        KEYBOARDBACKEND.get().unwrap().clone(),
        "/kernel/io/keyboard"
    );
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

impl_read_for_tty!(KeyboardBackend);
impl_empty_write!(KeyboardBackend);
impl_file_for_wr!(KeyboardBackend: NodeType::File);
