mod alloc;
mod frame;
mod map;
mod table;
//TODO make arch agnostic / abstract arch stuff away
use crate::{
    arch::{
        current_page_tbl,
        mem::{OffsetPageTable, PageTable, VirtAddr},
    },
    bootinfo,
};
pub use alloc::GLOBAL_FRAME_ALLOCATOR;
use lazy_static::lazy_static;
use spin::Mutex;

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
