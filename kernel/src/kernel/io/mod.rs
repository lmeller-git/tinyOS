use alloc::{string::String, vec::Vec};
use core::fmt;

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum IOError {
    #[error("IO Error TODO")]
    TODO,
}

pub type IOResult<T> = Result<T, IOError>;

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> IOResult<usize>;
    fn read_exact(&mut self, buf: &mut [u8]) -> IOResult<()> {
        todo!()
    }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> IOResult<usize> {
        todo!()
    }
    fn read_to_string(&mut self, buf: &mut String) -> IOResult<usize> {
        todo!()
    }
}

pub trait Write: fmt::Write {
    fn write(&self, buf: &[u8]) -> IOResult<usize>;
    fn write_all(&self, mut buf: &[u8]) -> IOResult<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => {
                    return Err(IOError::TODO);
                }
                Ok(n) => buf = &buf[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
