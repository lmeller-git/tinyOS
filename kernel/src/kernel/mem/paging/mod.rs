mod alloc;
//TODO make arch agnostic / abstract arch stuff away
use crate::bootinfo;
pub use alloc::GLOBAL_FRAME_ALLOCATOR;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::registers::control::Cr3;
use x86_64::{
    VirtAddr,
    structures::paging::{OffsetPageTable, PageTable},
};

unsafe fn active_level_4_table() -> &'static mut PageTable {
    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address().as_u64();
    // limine maps addresses above 4GiB
    let virt = if phys >= 0x100000000 {
        VirtAddr::new(bootinfo::get_phys_offset()) + phys
    } else {
        // not adding offset here leads to crash? TODO: check docs again
        VirtAddr::new(phys) + bootinfo::get_phys_offset()
    };
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

pub(super) fn init() {}
