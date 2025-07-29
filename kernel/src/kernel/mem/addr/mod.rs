#![allow(dead_code)]

mod phys;
mod virt;

use core::{
    fmt::{Debug, Display},
    ops::{Add, Shl, ShlAssign, Shr, ShrAssign, Sub},
};

pub use phys::PhysAddr;
pub use virt::VirtAddr;

use crate::bootinfo;

trait Addr:
    From<u64>
    + Into<u64>
    + Copy
    + Clone
    + Debug
    + Display
    + Default
    + Add<u64, Output = Self>
    + Sub<u64, Output = Self>
    + Shl<usize, Output = Self>
    + Shr<usize, Output = Self>
    + ShlAssign<usize>
    + ShrAssign<usize>
{
    fn into_inner(self) -> u64;

    fn new(addr: u64) -> Self;
}

pub fn virt_with_offset(addr: u64) -> VirtAddr {
    VirtAddr::new(addr + bootinfo::get_phys_offset())
}
