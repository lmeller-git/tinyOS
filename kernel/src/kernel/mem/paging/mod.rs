#![allow(dead_code)]

mod alloc;
mod frame;
mod map;
mod table;
pub use alloc::{GlobalFrameAllocator, get_frame_alloc, init_frame_alloc};
use core::{fmt::Debug, mem::ManuallyDrop};

use conquer_once::spin::OnceCell;
use lazy_static::lazy_static;
pub use map::{
    kernel_map_region,
    map_region,
    map_region_into,
    unmap_region,
    unmap_region_from,
    user_map_region,
};

//TODO make arch agnostic / abstract arch stuff away
use crate::{
    arch::{
        current_page_tbl,
        mem::{
            FrameAllocator,
            FrameDeallocator,
            Mapper,
            OffsetPageTable,
            Page,
            PageSize,
            PageTable,
            PhysFrame,
            Size4KiB,
            VirtAddr,
            mapper::CleanUp,
        },
    },
    bootinfo,
    kernel::mem::heap::map_heap,
    serial_println,
    sync::locks::Mutex,
};

pub const HIGHER_HALF_START: OnceCell<u64> = OnceCell::uninit();

pub fn get_hhdm_addr() -> u64 {
    *HIGHER_HALF_START.get_or_init(|| bootinfo::get_phys_offset())
}

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

    //copy higher half
    let hhdm = get_hhdm_addr() as usize;
    // extract lvl 4 pagetable offset
    let start_index = (hhdm >> 39) & 0x1ff;

    for i in start_index..512 {
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

impl TaskPageTable<'_> {
    /// SAFETY
    /// This method must be called from a different address space.
    /// The caller must ensure that no pointers into this address space remain at the point of calling this.
    /// This method may block.
    pub unsafe fn cleanup(mut self) {
        // TODO: ensure that NO ptrs into the dropped address space exist at this point

        // only unmap lower half
        let hhdm = get_hhdm_addr();

        let mut table = unsafe { ManuallyDrop::take(&mut self.table) };
        unsafe {
            table.clean_up_addr_range(
                Page::range_inclusive(
                    Page::containing_address(VirtAddr::zero()),
                    Page::containing_address(VirtAddr::new(hhdm - 1)),
                ),
                &mut *get_frame_alloc().lock(),
            );
        }
        unsafe {
            get_frame_alloc().lock().deallocate_frame(self.root);
        }
    }
}

impl Drop for TaskPageTable<'_> {
    fn drop(&mut self) {
        // TODO ensure this is cleaned up by now
    }
}

#[derive(Debug)]
pub enum APageTable<'a> {
    Global(&'a Mutex<OffsetPageTable<'a>>),
    Owned(Mutex<TaskPageTable<'a>>),
}

impl APageTable<'static> {
    pub fn global() -> Self {
        Self::Global(&PAGETABLE)
    }
}

impl<'a> APageTable<'a> {
    pub fn owned(table: Mutex<TaskPageTable<'a>>) -> Self {
        Self::Owned(table)
    }

    /// SAFETY
    /// This method must be called from a different address space.
    /// The caller must ensure that no pointers into this address space remain at the point of calling this.
    /// This method may block.
    pub unsafe fn cleanup(mut self) {
        match self {
            Self::Global(_) => {}
            Self::Owned(mut table) => unsafe { table.into_inner().cleanup() },
        }
    }

    pub fn clone(&self) -> Self {
        // This should lazily copy the pagedirs, using Cow.
        // Currently this is not possible, thus we eagerly duplicate all (non-global) mappings
        match self {
            Self::Global(g) => Self::Global(g),
            Self::Owned(o) => todo!(),
        }
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
impl<'a> Mapper<Size4KiB> for APageTable<'a> {
    unsafe fn map_to<A>(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        frame: PhysFrame<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
        frame_allocator: &mut A,
    ) -> Result<
        x86_64::structures::paging::mapper::MapperFlush<Size4KiB>,
        x86_64::structures::paging::mapper::MapToError<Size4KiB>,
    >
    where
        Self: Sized,
        A: FrameAllocator<Size4KiB> + ?Sized,
    {
        match self {
            Self::Global(m) => m.lock().map_to(page, frame, flags, frame_allocator),
            Self::Owned(m) => m.lock().table.map_to(page, frame, flags, frame_allocator),
        }
    }

    unsafe fn map_to_with_table_flags<A>(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        frame: PhysFrame<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
        parent_table_flags: x86_64::structures::paging::PageTableFlags,
        frame_allocator: &mut A,
    ) -> Result<
        x86_64::structures::paging::mapper::MapperFlush<Size4KiB>,
        x86_64::structures::paging::mapper::MapToError<Size4KiB>,
    >
    where
        Self: Sized,
        A: FrameAllocator<Size4KiB> + ?Sized,
    {
        match self {
            Self::Global(m) => m.lock().map_to_with_table_flags(
                page,
                frame,
                flags,
                parent_table_flags,
                frame_allocator,
            ),
            Self::Owned(m) => m.lock().table.map_to_with_table_flags(
                page,
                frame,
                flags,
                parent_table_flags,
                frame_allocator,
            ),
        }
    }

    fn unmap(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
    ) -> Result<
        (
            PhysFrame<Size4KiB>,
            x86_64::structures::paging::mapper::MapperFlush<Size4KiB>,
        ),
        x86_64::structures::paging::mapper::UnmapError,
    > {
        match self {
            Self::Global(m) => m.lock().unmap(page),
            Self::Owned(m) => m.lock().table.unmap(page),
        }
    }

    unsafe fn update_flags(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
    ) -> Result<
        x86_64::structures::paging::mapper::MapperFlush<Size4KiB>,
        x86_64::structures::paging::mapper::FlagUpdateError,
    > {
        match self {
            Self::Global(m) => m.lock().update_flags(page, flags),
            Self::Owned(m) => m.lock().table.update_flags(page, flags),
        }
    }

    unsafe fn set_flags_p4_entry(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
    ) -> Result<
        x86_64::structures::paging::mapper::MapperFlushAll,
        x86_64::structures::paging::mapper::FlagUpdateError,
    > {
        match self {
            Self::Global(m) => m.lock().set_flags_p4_entry(page, flags),
            Self::Owned(m) => m.lock().table.set_flags_p4_entry(page, flags),
        }
    }

    unsafe fn set_flags_p3_entry(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
    ) -> Result<
        x86_64::structures::paging::mapper::MapperFlushAll,
        x86_64::structures::paging::mapper::FlagUpdateError,
    > {
        match self {
            Self::Global(m) => m.lock().set_flags_p3_entry(page, flags),
            Self::Owned(m) => m.lock().table.set_flags_p3_entry(page, flags),
        }
    }

    unsafe fn set_flags_p2_entry(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
    ) -> Result<
        x86_64::structures::paging::mapper::MapperFlushAll,
        x86_64::structures::paging::mapper::FlagUpdateError,
    > {
        match self {
            Self::Global(m) => m.lock().set_flags_p2_entry(page, flags),
            Self::Owned(m) => m.lock().table.set_flags_p2_entry(page, flags),
        }
    }

    fn translate_page(
        &self,
        page: x86_64::structures::paging::Page<Size4KiB>,
    ) -> Result<PhysFrame<Size4KiB>, x86_64::structures::paging::mapper::TranslateError> {
        match self {
            Self::Global(m) => m.lock().translate_page(page),
            Self::Owned(m) => m.lock().table.translate_page(page),
        }
    }
}
