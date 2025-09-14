use alloc::{string::String, vec::Vec};
use core::fmt;

use thiserror::Error;

use crate::kernel::fs::FSError;

pub type IOError = FSError;
pub type IOResult<T> = Result<T, IOError>;

pub trait Read {
    fn read(&self, buf: &mut [u8], offset: usize) -> IOResult<usize>;
    fn read_exact(&self, buf: &mut [u8], offset: usize) -> IOResult<()> {
        todo!()
    }
    fn read_to_end(&self, buf: &mut Vec<u8>, offset: usize) -> IOResult<usize> {
        todo!()
    }
    fn read_to_string(&self, buf: &mut String, offset: usize) -> IOResult<usize> {
        todo!()
    }
}

pub trait Write {
    fn write(&self, buf: &[u8], offset: usize) -> IOResult<usize>;
    fn write_all(&self, mut buf: &[u8], mut offset: usize) -> IOResult<()> {
        while !buf.is_empty() {
            match self.write(buf, offset) {
                Ok(0) => {
                    return Err(IOError::simple(super::fs::FSErrorKind::Other));
                }
                Ok(n) => {
                    buf = &buf[n..];
                    offset += n
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
