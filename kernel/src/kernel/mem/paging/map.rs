use alloc::{format, string::String};

use crate::{
    arch::mem::{
        FrameAllocator,
        FrameDeallocator,
        Mapper,
        Page,
        PageSize,
        PageTableFlags,
        PhysAddr,
        PhysFrame,
        Size2MiB,
        Size4KiB,
        VirtAddr,
        mapper,
    },
    bootinfo::get_phys_offset,
    eprintln,
    kernel::{
        mem::{
            addr::{PhysAddr as paddr, VirtAddr as vaddr},
            paging::{PAGETABLE, get_frame_alloc, get_hhdm_addr},
        },
        threading::{task::TaskRepr, tls},
    },
    serial_println,
};

pub struct PageTableMapper {}

impl PageTableMapper {
    fn new() -> Self {
        Self {}
    }

    fn map_to(&self, virt: vaddr, phys: paddr) {}

    fn map_any(&self, phys: paddr) {}
}

// TODO all following functions should try to clean up after themselves in case of an error.

pub fn user_map_region(start: VirtAddr, len: usize) -> Result<(), &'static str> {
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE;
    map_region(
        start,
        len,
        flags,
        tls::task_data().current_thread().unwrap().pagedir(),
    )
}

pub fn kernel_map_region(start: VirtAddr, len: usize) -> Result<(), &'static str> {
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    map_region(start, len, flags, &mut *PAGETABLE.lock())
}

/// maps a NEW region from start..start + len into the provided address space. NEW memory will be allocated for this process.
/// Thus len should be the len in BYTES.
pub fn map_region<M: Mapper<Size4KiB>>(
    start: VirtAddr,
    len: usize,
    flags: PageTableFlags,
    pagetable: &mut M,
) -> Result<(), &'static str> {
    assert!(flags.contains(PageTableFlags::PRESENT));
    let end_addr = (start + len as u64).align_up(Size4KiB::SIZE);
    let start = Page::containing_address(start);
    let end = Page::containing_address(end_addr);
    let mut alloc = get_frame_alloc().lock();
    let range = Page::range(start.clone(), end.clone());
    let n = range.count();

    for page in Page::range(start, end) {
        if pagetable.translate_page(page).is_ok() {
            return Err("a memory region was already mapped, but we tried to map it again.");
        }
        let frame = alloc
            .allocate_frame()
            .ok_or::<&str>("could not allocate frame")?;
        unsafe { pagetable.map_to(page, frame, flags, &mut *alloc) }
            .map_err(|_e| "map failed during map_to")?
            .flush();
    }
    Ok(())
}

/// unmaps a region from start..start + len from the provided address space and frees the underlying memory.
/// len should be in BYTES.
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

/// maps a region from start..start + len into the provided address space. This does not allocate any memory.
/// Thus len should be the len in BYTES.
/// Returns the new address in address space pagetable, where from is mapped to.
/// This address is not guaranteed to be page aligned.
pub fn map_region_into<M: Mapper<Size4KiB>, M2: Mapper<Size4KiB>>(
    start: VirtAddr,
    len: usize,
    flags: PageTableFlags,
    pagetable: &mut M,
    from: VirtAddr,
    from_addr_space: &mut M2,
) -> Result<VirtAddr, &'static str> {
    assert!(flags.contains(PageTableFlags::PRESENT));
    let end_addr = (start + len as u64);
    let start_page = Page::containing_address(start);
    let end = Page::containing_address(end_addr);
    let from_start = Page::containing_address(from);
    let from_end_addr = (from + len as u64);
    let from_end = Page::containing_address(from_end_addr);
    let down_aligned = from.align_down(Size4KiB::SIZE);
    let offset_to_page_start = from - down_aligned;
    let mapped_addr = start + offset_to_page_start;
    let mut mapped_so_far = 0;

    serial_println!(
        "trying to map {} pages. donor: {:#x}..{:#x}",
        len as u64 / Size4KiB::SIZE,
        from,
        from_end_addr
    );

    let (mut from_iter, mut to_iter) = (
        Page::range(from_start, from_end),
        Page::range(start_page, end),
    );

    // for (from_page, to_page) in Page::range(from_start, from_end).zip(Page::range(start_page, end))
    while let (Some(from_page), Some(to_page)) = (from_iter.next(), to_iter.next()) {
        if pagetable.translate_page(to_page).is_ok() {
            eprintln!("a memory region was already mapped, but we tried to map it again.");
            return Err("a memory region was already mapped, but we tried to map it again.");
        }

        match from_addr_space.translate_page(from_page) {
            Ok(frame) => {
                unsafe { pagetable.map_to(to_page, frame, flags, &mut *get_frame_alloc().lock()) }
                    .map_err(|_| "failed to map a page")?
                    .flush();
                mapped_so_far += Size4KiB::SIZE;
            }
            Err(e) => match e {
                mapper::TranslateError::ParentEntryHugePage => {
                    let start_addr = from_page.start_address();
                    if start_addr.as_u64() < get_hhdm_addr() {
                        eprintln!(
                            "Huge Page below identity mapped memory, currently cannot handle this. Aborting..."
                        );
                        return Err("Huge Page below identity mapped memory");
                    }
                    // the address is in the higher half and thus identity mapped.
                    // We can retrieve the actual addresses using physical_offset.
                    // We will map a number of regular pages, until we have mapped either the whole Huge Page, or len. If len > Huge Page size, we will continue.
                    // This is done, because currently our mapper does not implement Mapper<Size2MiB>. TODO This should be added in the future.
                    // A Huge Page has a size of 2MiB.
                    let phys_frame_start_addr = start_addr.as_u64() - get_phys_offset();
                    let phys_frame_end_addr = start_addr.as_u64() - get_phys_offset()
                        + Size2MiB::SIZE.min(len as u64 - mapped_so_far);
                    let n_pages = Size2MiB::SIZE.min(len as u64 - mapped_so_far) / Size4KiB::SIZE;

                    serial_println!("mapping {} pages", n_pages);

                    for (i, frame) in PhysFrame::range(
                        PhysFrame::containing_address(PhysAddr::new(phys_frame_start_addr)),
                        PhysFrame::containing_address(PhysAddr::new(phys_frame_end_addr)),
                    )
                    .enumerate()
                    {
                        let virt_addr = to_page.start_address() + i as u64 * Size4KiB::SIZE;
                        let page = Page::containing_address(virt_addr);
                        unsafe {
                            pagetable.map_to(page, frame, flags, &mut *get_frame_alloc().lock())
                        }
                        .map_err(|_| "failed to map page during huge page mapping")?
                        .flush();
                    }
                    mapped_so_far += Size2MiB::SIZE.min(len as u64 - mapped_so_far);
                    // first page already consumed
                    for _ in 1..n_pages as usize {
                        to_iter.next();
                        from_iter.next();
                    }
                }
                _e => {
                    serial_println!(
                        "err at pages to: {:#x}, from: {:#?}",
                        to_page.start_address(),
                        from_page.start_address()
                    );
                    return Err("a page was not found in the donor address space");
                }
            },
        }
    }
    Ok(mapped_addr)
}

/// unmaps a region from start..start + len from the provided address space. The underlying memory is NOT freed.
/// len should be in BYTES.
pub fn unmap_region_from<M: Mapper<Size4KiB>>(
    start: VirtAddr,
    len: usize,
    pagetable: &mut M,
) -> Result<(), String> {
    let end_addr = (start + len as u64).align_up(Size4KiB::SIZE);
    let start = Page::containing_address(start);
    let end = Page::containing_address(end_addr);

    for page in Page::range_inclusive(start, end) {
        let (frame, flush) = pagetable.unmap(page).map_err(|e| format!("{:?}", e))?;
        flush.flush();
    }
    Ok(())
}
