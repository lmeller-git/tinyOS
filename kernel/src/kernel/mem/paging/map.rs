use alloc::format;
use alloc::string::String;
use x86_64::structures::paging::FrameAllocator;

use crate::arch::mem::{Mapper, Page, PageSize, PageTableFlags, Size4KiB, VirtAddr};
use crate::kernel::mem::addr::{PhysAddr as paddr, VirtAddr as vaddr};
use crate::kernel::mem::paging::{GLOBAL_FRAME_ALLOCATOR, PAGETABLE};
use crate::kernel::threading;
use crate::kernel::threading::schedule::{current_task, with_current_task};
use crate::kernel::threading::task::{PrivilegeLevel, TaskRepr};
use crate::serial_println;

pub struct PageTableMapper {}

impl PageTableMapper {
    fn new() -> Self {
        Self {}
    }

    fn map_to(&self, virt: vaddr, phys: paddr) {}

    fn map_any(&self, phys: paddr) {}
}

pub fn user_map_region(start: VirtAddr, len: usize) -> Result<(), String> {
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE;
    map_region(start, len, flags)
}

pub fn kernel_map_region(start: VirtAddr, len: usize) -> Result<(), String> {
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    map_region(start, len, flags)
}

fn map_region(start: VirtAddr, len: usize, flags: PageTableFlags) -> Result<(), String> {
    let end_addr = (start + len as u64).align_up(Size4KiB::SIZE);
    let start = Page::containing_address(start);
    let end = Page::containing_address(end_addr);
    if !threading::is_running()
        || current_task().unwrap().raw().read().privilege_level() == PrivilegeLevel::Kernel
    {
        let mut pagedir = PAGETABLE.lock();
        let mut alloc = GLOBAL_FRAME_ALLOCATOR.lock();
        for page in Page::range_inclusive(start, end) {
            let frame = alloc
                .allocate_frame()
                .ok_or::<String>("could not allocate frame".into())?;
            unsafe { pagedir.map_to(page, frame, flags, &mut *alloc) }
                .map_err(|e| format!("{:?}", e))?
                .flush();
        }
        Ok(())
    } else {
        with_current_task(|task| {
            task.with_inner_mut(|task| {
                let pagedir = task.mut_pagdir();
                let mut alloc = GLOBAL_FRAME_ALLOCATOR.lock();
                for page in Page::range_inclusive(start, end) {
                    let frame = alloc
                        .allocate_frame()
                        .ok_or::<String>("could not allocate frame".into())?;
                    unsafe { pagedir.table.map_to(page, frame, flags, &mut *alloc) }
                        .map_err(|e| format!("{:?}", e))?
                        .flush();
                }
                Ok(())
            })
        })
        .ok_or::<String>("could not access task".into())?
    }
}
