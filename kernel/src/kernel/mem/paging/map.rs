use crate::kernel::mem::addr::{PhysAddr, VirtAddr};

pub struct PageTableMapper {}

impl PageTableMapper {
    fn new() -> Self {
        Self {}
    }

    fn map_to(&self, virt: VirtAddr, phys: PhysAddr) {}

    fn map_any(&self, phys: PhysAddr) {}
}
