use alloc::{format, string::String};

use crate::{
    arch::mem::{
        FrameAllocator,
        FrameDeallocator,
        Mapper,
        Page,
        PageSize,
        PageTableFlags,
        Size4KiB,
        VirtAddr,
    },
    kernel::{
        mem::{
            addr::{PhysAddr as paddr, VirtAddr as vaddr},
            paging::{PAGETABLE, get_frame_alloc},
        },
        threading::{task::TaskRepr, tls},
    },
};

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
    map_region(
        start,
        len,
        flags,
        tls::task_data().get_current().unwrap().pagedir(),
    )
}

pub fn kernel_map_region(start: VirtAddr, len: usize) -> Result<(), String> {
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    map_region(start, len, flags, &mut *PAGETABLE.lock())
}

/// maps a region from start..start + len into the provided address space.
/// Thus len should be the len in BYTES.
pub fn map_region<M: Mapper<Size4KiB>>(
    start: VirtAddr,
    len: usize,
    flags: PageTableFlags,
    pagetable: &mut M,
) -> Result<(), String> {
    assert!(flags.contains(PageTableFlags::PRESENT));
    let end_addr = (start + len as u64).align_up(Size4KiB::SIZE);
    let start = Page::containing_address(start);
    let end = Page::containing_address(end_addr);
    let mut alloc = get_frame_alloc().lock();
    for page in Page::range_inclusive(start, end) {
        if pagetable.translate_page(page).is_ok() {
            continue;
        }
        let frame = alloc
            .allocate_frame()
            .ok_or::<String>("could not allocate frame".into())?;
        unsafe { pagetable.map_to(page, frame, flags, &mut *alloc) }
            .map_err(|e| format!("{:?}", e))?
            .flush();
    }
    Ok(())
}

pub fn unmap_region<M: Mapper<Size4KiB>>(
    start: VirtAddr,
    len: usize,
    pagetable: &mut M,
) -> Result<(), String> {
    let end_addr = (start + len as u64).align_up(Size4KiB::SIZE);
    let start = Page::containing_address(start);
    let end = Page::containing_address(end_addr);

    let mut alloc = get_frame_alloc().lock();
    for page in Page::range_inclusive(start, end) {
        let (frame, flush) = pagetable.unmap(page).map_err(|e| format!("{:?}", e))?;
        flush.flush();
        unsafe { alloc.deallocate_frame(frame) };
    }
    Ok(())
}
