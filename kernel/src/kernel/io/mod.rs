use alloc::{string::String, vec::Vec};

use crate::kernel::fs::{FSError, FSErrorKind};

pub type IOError = FSError;
pub type IOResult<T> = Result<T, IOError>;

pub trait Read {
    fn read(&self, buf: &mut [u8], offset: usize) -> IOResult<usize>;
    fn read_exact(&self, buf: &mut [u8], offset: usize) -> IOResult<()> {
        todo!()
    }

    fn read_to_end(&self, buf: &mut Vec<u8>, mut offset: usize) -> IOResult<usize> {
        let mut written = 0;
        loop {
            let count = self.read(&mut buf[written..], offset)?;
            if count == buf[written..].len() {
                buf.resize(buf.len().max(1) * 2, 0);
            } else if count == 0 {
                return Ok(written);
            }
            written += count;
            offset += count;
        }
    }

    fn read_to_string(&self, buf: &mut String, offset: usize) -> IOResult<usize> {
        let mut buff = Vec::new();
        let count = self.read_to_end(&mut buff, 0)?;
        debug_assert!(count <= buff.len());
        let str_ =
            str::from_utf8(&buff[..count]).map_err(|_| IOError::simple(FSErrorKind::Other))?;
        buf.extend(str_.chars());
        Ok(str_.len())
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
