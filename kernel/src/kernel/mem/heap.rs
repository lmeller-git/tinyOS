use super::{alloc, paging};
use crate::arch::mem::{FrameAllocator, Mapper, Page, PageTableFlags, VirtAddr};

pub const HEAP_START: usize = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 200 * 1024; // 200 KiB

pub fn init() {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };
    for page in page_range {
        let mut allocator = paging::GLOBAL_FRAME_ALLOCATOR.lock();
        let frame = allocator.allocate_frame().unwrap();
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            paging::PAGETABLE
                .lock()
                .map_to(page, frame, flags, &mut *allocator)
                .unwrap()
                .flush();
        };
    }
    unsafe {
        alloc::GLOBAL_ALLOCATOR.init(HEAP_START as *mut u8, HEAP_SIZE);
    }
}

pub fn map_heap(tbl: &mut crate::arch::mem::OffsetPageTable) {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page: Page<crate::arch::mem::Size4KiB> =
            Page::containing_address(heap_start);
        let heap_end_page: Page<crate::arch::mem::Size4KiB> = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    let mapper = paging::PAGETABLE.lock();
    let mut frame_allocator = paging::GLOBAL_FRAME_ALLOCATOR.lock();
    for page in page_range {
        let frame = mapper.translate_page(page).unwrap();
        unsafe {
            tbl.map_to(
                page,
                frame,
                PageTableFlags::WRITABLE | PageTableFlags::PRESENT,
                &mut *frame_allocator,
            )
            .unwrap()
            .flush();
        }
    }
}
