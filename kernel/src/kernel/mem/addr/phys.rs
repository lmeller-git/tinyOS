use os_macros::Addr;

use super::Addr;

#[repr(transparent)]
#[derive(Addr, Clone, Copy, Debug)]
pub struct PhysAddr {
    inner: u64,
}

impl PhysAddr {}
