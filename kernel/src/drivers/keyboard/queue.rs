use crossbeam::queue::ArrayQueue;
use lazy_static::lazy_static;

use super::KeyboardError;
use crate::{
    arch::interrupt,
    kernel::threading::{
        self,
        task::{TaskID, TaskRepr},
        tls,
    },
    serial_println,
};

pub struct KeyboardBuffer {
    inner: ArrayQueue<u8>,
}

impl KeyboardBuffer {
    fn new() -> Self {
        Self {
            inner: ArrayQueue::new(20),
        }
    }

    pub fn put(&self, element: u8) -> Result<(), KeyboardError> {
        self.inner
            .push(element)
            .map_err(|_| KeyboardError::FullQueue)?;
        Ok(())
    }

    pub fn read(&self) -> Result<u8, KeyboardError> {
        // only use if you really must
        let s = self.inner.pop().ok_or(KeyboardError::EmptyQueue)?;
        let new = ArrayQueue::new(10);
        new.force_push(s);
        while let Some(v) = self.inner.pop() {
            new.force_push(v);
        }
        while let Some(v) = new.pop() {
            self.inner.force_push(v);
        }

        Ok(s)
    }

    pub fn pop(&self) -> Result<u8, KeyboardError> {
        self.inner.pop().ok_or(KeyboardError::EmptyQueue)
    }

    pub fn clear(&self) {
        while self.inner.pop().is_some() {}
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

pub fn put_scancode(code: u8) -> Result<(), KeyboardError> {
    KEYBOARD_BUFFER.put(code)
}

pub fn get_current_next() -> Result<u8, KeyboardError> {
    KEYBOARD_BUFFER.pop()
}

pub fn get_next() -> u8 {
    loop {
        if !KEYBOARD_BUFFER.inner.is_empty() {
            return KEYBOARD_BUFFER.pop().unwrap();
        }
    }
}

lazy_static! {
    pub static ref KEYBOARD_BUFFER: KeyboardBuffer = KeyboardBuffer::new();
}
