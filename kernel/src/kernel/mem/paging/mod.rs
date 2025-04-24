mod alloc;
mod frame;
mod map;
mod table;
//TODO make arch agnostic / abstract arch stuff away
use crate::bootinfo;
pub use alloc::GLOBAL_FRAME_ALLOCATOR;
use lazy_static::lazy_static;
use spin::Mutex;
// use x86_64::PhysAddr;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::RecursivePageTable;
// use x86_64::structures::paging::{Mapper, Size4KiB};
// use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame};
use x86_64::{
    VirtAddr,
    structures::paging::{OffsetPageTable, PageTable},
};

// fn map_vga() {
//     let vga_phys = PhysAddr::new(0xb8000);
//     let vga_virt = VirtAddr::new(bootinfo::get_phys_offset() + 0xb8000);

//     let page: x86_64::structures::paging::Page<Size4KiB> = Page::containing_address(vga_virt);
//     let frame = PhysFrame::containing_address(vga_phys);
//     let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
//     let mut allocator = GLOBAL_FRAME_ALLOCATOR.lock();
//     unsafe {
//         PAGETABLE
//             .lock()
//             .map_to(page, frame, flags, &mut *allocator)
//             .unwrap()
//             .flush();
//     }
// }

// reads current p4 rom cpu (CR3) and returns pointer
unsafe fn active_level_4_table() -> &'static mut PageTable {
    let (level_4_table_frame, _) = Cr3::read();
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

// current bootloader p4 table not recursive
// lazy_static! {
//     pub static ref PAGETABLE: Mutex<RecursivePageTable<'static>> = {
//         let level_4_table = unsafe { active_level_4_table() };
//         unsafe { Mutex::new(RecursivePageTable::new(level_4_table).unwrap()) }
//     };
// }

pub(super) fn init() {
    // map_vga();
}
