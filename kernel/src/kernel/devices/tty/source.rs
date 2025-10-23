use alloc::sync::Arc;

use conquer_once::spin::OnceCell;

use super::TTYSource;
use crate::{
    drivers::{
        keyboard::{get_current_next, parse_scancode},
        tty::map_key,
    },
    impl_empty_write,
    impl_file_for_wr,
    impl_read_for_tty,
    kernel::{devices::tty::TTYSink, fs::NodeType},
    register_device_file,
    serial_println,
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

    fn read_buf(&self, mut buf: &mut [u8], _offset: usize) -> crate::kernel::io::IOResult<usize> {
        let mut intermediate_buf = alloc::vec![0; buf.len()];

        let mut n_read = 0;
        let mut buf_iter = intermediate_buf.iter_mut();
        while let Some(next_idx) = buf_iter.next()
            && let Some(read) = self.read()
        {
            *next_idx = read;
            n_read += 1;
        }

        let mut n_mapped = 0;
        for &byte in &intermediate_buf[..n_read] {
            if let Ok(res) = parse_scancode(byte) {
                let mapped_bytes = map_key(res, buf);
                if mapped_bytes < 0 {
                    break;
                }
                buf = &mut buf[mapped_bytes as usize..];
                n_mapped += mapped_bytes as usize;
            }
        }
        Ok(n_mapped)
    }
}

impl_read_for_tty!(KeyboardBackend);
impl_empty_write!(KeyboardBackend);
impl_file_for_wr!(KeyboardBackend: NodeType::File);
