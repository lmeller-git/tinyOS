use core::{
    array,
    cell::UnsafeCell,
    sync::atomic::{AtomicU8, AtomicUsize, Ordering},
};

use crossbeam::queue::ArrayQueue;
use lazy_static::lazy_static;

use super::KeyboardError;

pub const STDIN_QUEUE_SIZE: usize = 50;

pub struct KeyboardBuffer {
    inner: [AtomicU8; STDIN_QUEUE_SIZE],
    count: AtomicUsize,
}

impl KeyboardBuffer {
    fn new() -> Self {
        Self {
            inner: array::from_fn(|_| 0.into()),
            count: 0.into(),
        }
    }

    pub fn put(&self, element: u8) {
        let idx = self.count.load(Ordering::Acquire) % STDIN_QUEUE_SIZE;
        self.inner
            .get(idx)
            .unwrap()
            .store(element, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Release);
    }

    pub fn read1(&self, cursor: usize) -> Option<u8> {
        let current = self.count.load(Ordering::Acquire);
        if self.cursor_is_valid(cursor) {
            let idx = cursor % STDIN_QUEUE_SIZE;
            Some(self.inner.get(idx).unwrap().load(Ordering::Relaxed))
        } else {
            None
        }
    }

    pub fn readn(&self, mut cursor: usize, buf: &mut [u8]) -> usize {
        let mut n = 0;
        while self.cursor_is_valid(cursor) && n < buf.len() {
            buf[n] = self
                .inner
                .get(cursor % STDIN_QUEUE_SIZE)
                .unwrap()
                .load(Ordering::Relaxed);
            n += 1;
            cursor += 1;
        }
        n
    }

    pub fn get_current(&self) -> usize {
        self.count.load(Ordering::Acquire) - 1
    }

    pub fn cursor_is_valid(&self, cursor: usize) -> bool {
        let current = self.count.load(Ordering::Acquire);
        (current.saturating_sub(STDIN_QUEUE_SIZE)..current).contains(&cursor)
    }

    pub fn is_up_to_date(&self, cursor: usize) -> bool {
        self.count.load(Ordering::Acquire) == cursor
    }

    pub fn clear(&self) {
        self.count.store(0, Ordering::Release);
    }

    pub fn is_empty(&self) -> bool {
        self.count.load(Ordering::Relaxed) == 0
    }
}

pub fn put_scancode(code: u8) {
    KEYBOARD_BUFFER.put(code)
}

unsafe impl Sync for KeyboardBuffer {}
unsafe impl Send for KeyboardBuffer {}

lazy_static! {
    pub static ref KEYBOARD_BUFFER: KeyboardBuffer = KeyboardBuffer::new();
}
