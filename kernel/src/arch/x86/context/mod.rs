use lazy_static::lazy_static;
use x86_64::structures::paging::FrameDeallocator;

use crate::{
    arch::{
        current_page_tbl,
        mem::{FrameAllocator, Mapper, Page, PageSize, PageTableFlags, Size4KiB, VirtAddr},
    },
    kernel::{
        mem::paging::{GLOBAL_FRAME_ALLOCATOR, PAGETABLE},
        threading::ThreadingError,
    },
};
use spin::Mutex;

const KSTACK_AREA_START: VirtAddr = VirtAddr::new(0xffff_8000_1000_0000);
const KSTACK_AREA_SIZE: usize = 64 * 1024 * 1024; // 64MiB
const KSTACK_SIZE: usize = 16 * 1024; // 16KiB
const MAX_KSTACKS: usize = KSTACK_AREA_SIZE / KSTACK_SIZE;

lazy_static! {
    static ref KSTACKS_IN_USAGE: Mutex<[bool; MAX_KSTACKS]> = Mutex::new([false; MAX_KSTACKS]);
}

#[derive(Default)]
#[repr(C)]
pub struct TaskCtx {
    rsp: u64,
    rflags: u64,
    ss: u64,
    cs: u64,

    rip: u64,

    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,

    rbp: u64,
    rdi: u64,
    rsi: u64,

    rdx: u64,
    rcx: u64,
    rbx: u64,
    cr3: u64,
    rax: u64,
}

impl TaskCtx {
    pub fn new(entry: usize, arg: usize, stack_top: VirtAddr) -> Self {
        Self {
            rsp: stack_top.as_u64(),
            rflags: 0x202,
            rip: entry as u64,
            ..Default::default()
        }
    }
}

#[inline(always)]
fn context_switch(ctx: &TaskCtx) -> ! {
    loop {}
}

pub fn allocate_kstack() -> Result<VirtAddr, ThreadingError> {
    let (current_table, current_flags) = current_page_tbl();
    let flags = PageTableFlags::WRITABLE | PageTableFlags::PRESENT;
    let mut i = 0;

    let kstack_start_idx = {
        let mut in_use = KSTACKS_IN_USAGE.lock();
        in_use
            .iter_mut()
            .position(|pos| {
                if !*pos {
                    *pos = true;
                    true
                } else {
                    false
                }
            })
            .ok_or(ThreadingError::StackNotBuilt)?
    };

    //TODO need to make sure these are aligned
    let start = KSTACK_AREA_START + kstack_start_idx as u64 * KSTACK_SIZE as u64 + Size4KiB::SIZE;
    let end = start - 2 * Size4KiB::SIZE + KSTACK_SIZE as u64;

    let start_page = Page::containing_address(start);
    let end_page = Page::containing_address(end);

    {
        let mut mapper = PAGETABLE.lock();
        let mut frame_allocator = GLOBAL_FRAME_ALLOCATOR.lock();

        for page in Page::range_inclusive(start_page, end_page) {
            let frame = frame_allocator
                .allocate_frame()
                .ok_or(ThreadingError::StackNotBuilt)?;
            unsafe {
                mapper
                    .map_to(page, frame, flags, &mut *frame_allocator)
                    .map_err(|_| ThreadingError::StackNotBuilt)?
                    .flush();
            };
        }
    }
    Ok(end + Size4KiB::SIZE)
}

pub fn free_kstack(top: VirtAddr) -> Result<(), ThreadingError> {
    let start = top - KSTACK_SIZE as u64;
    let idx = (start - KSTACK_AREA_START) as usize / KSTACK_SIZE;
    KSTACKS_IN_USAGE
        .lock()
        .get_mut(idx)
        .ok_or(ThreadingError::StackNotFreed)?;
    // Alignment?
    let start_page = Page::containing_address(start + Size4KiB::SIZE);
    let end_page = Page::containing_address(top - Size4KiB::SIZE);
    {
        let mut mapper = PAGETABLE.lock();
        let mut frame_allocator = GLOBAL_FRAME_ALLOCATOR.lock();
        for page in Page::range_inclusive(start_page, end_page) {
            let (frame, flush) = mapper
                .unmap(page)
                .map_err(|_| ThreadingError::StackNotFreed)?;
            flush.flush();
            unsafe { frame_allocator.deallocate_frame(frame) };
        }
    }
    Ok(())
}
