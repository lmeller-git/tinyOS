use alloc::{sync::Arc, vec::Vec};
use core::fmt;

use crate::{
    kernel::{
        fd::IOCapable,
        io::{Read, Write},
    },
    sync::locks::RwLock,
};

struct RamNode {
    data: Arc<RwLock<Vec<u8>>>,
}

impl IOCapable for RamNode {}

impl Read for RamNode {
    fn read(&mut self, buf: &mut [u8]) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl Write for RamNode {
    fn write(&self, buf: &[u8]) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl fmt::Write for RamNode {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        todo!()
    }
}
