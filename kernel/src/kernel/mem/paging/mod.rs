#![allow(dead_code)]

mod alloc;
mod frame;
mod map;
mod table;
pub use alloc::{GlobalFrameAllocator, get_frame_alloc, init_frame_alloc};
use core::{fmt::Debug, mem::ManuallyDrop};

use lazy_static::lazy_static;
pub use map::{kernel_map_region, map_region, unmap_region, user_map_region};
use spin::Mutex;

//TODO make arch agnostic / abstract arch stuff away
use crate::{
    arch::{
        current_page_tbl,
        mem::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, VirtAddr},
    },
    bootinfo,
    kernel::mem::heap::map_heap,
};

// reads current p4 rom cpu (CR3) and returns pointer
unsafe fn active_level_4_table() -> &'static mut PageTable {
    let (level_4_table_frame, _) = current_page_tbl();
    let phys = level_4_table_frame.start_address().as_u64();
    let virt = VirtAddr::new(bootinfo::get_phys_offset() + phys);
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    unsafe { &mut *page_table_ptr }
}

// SAFETY: this depends on the safety of physical mem offset
lazy_static! {
    pub static ref PAGETABLE: Mutex<OffsetPageTable<'static>> = {
        let offset = bootinfo::get_phys_offset();
        let level_4_table = unsafe { active_level_4_table() };
        unsafe { Mutex::new(OffsetPageTable::new(level_4_table, VirtAddr::new(offset))) }
    };
}

pub struct TaskPageTable<'a> {
    pub table: ManuallyDrop<OffsetPageTable<'a>>,
    pub root: PhysFrame,
}

pub fn create_new_pagedir<'a, 'b>() -> Result<TaskPageTable<'b>, &'a str> {
    let new_frame = get_frame_alloc()
        .lock()
        .allocate_frame()
        .ok_or("no frame available")?;

    let new_tbl_ptr =
        VirtAddr::new(new_frame.start_address().as_u64() + bootinfo::get_phys_offset());
    let new_table: &mut PageTable = unsafe { &mut *(new_tbl_ptr.as_mut_ptr()) };
    new_table.zero();

    let (current_frame, _) = current_page_tbl();
    let current_tbl_ptr =
        VirtAddr::new(current_frame.start_address().as_u64() + bootinfo::get_phys_offset());
    let current_tbl: &PageTable = unsafe { &*(current_tbl_ptr.as_mut_ptr()) };

    // let flags = PageTableFlags::PRESENT
    // | PageTableFlags::WRITABLE
    // | PageTableFlags::USER_ACCESSIBLE
    // | PageTableFlags::NO_EXECUTE;

    //copy higher half
    for i in 256..512 {
        new_table[i] = current_tbl[i].clone();
    }

    let mut new_offset_page_tbl = ManuallyDrop::new(unsafe {
        OffsetPageTable::new(new_table, VirtAddr::new(bootinfo::get_phys_offset()))
    });

    map_heap(&mut new_offset_page_tbl);

    Ok(TaskPageTable {
        table: new_offset_page_tbl,
        root: new_frame,
    })
}

impl Debug for TaskPageTable<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Ok(())
    }
}

impl Drop for TaskPageTable<'_> {
    fn drop(&mut self) {
        // unsafe { ManuallyDrop::drop(self.table) };
    }
}
