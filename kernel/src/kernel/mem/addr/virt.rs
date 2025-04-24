use os_macros::Addr;

use super::{Addr, PhysAddr};

#[repr(transparent)]
#[derive(Addr, Clone, Copy, Debug)]
pub struct VirtAddr {
    inner: u64,
}

impl VirtAddr {
    fn from_phys(phys: PhysAddr) -> Self {
        Self {
            inner: phys.into_inner(),
        }
    }
}
