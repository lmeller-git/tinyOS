use core::ptr::null_mut;

use conquer_once::spin::OnceCell;

use crate::{
    arch::mem::{
        FrameAllocator,
        FrameDeallocator,
        PageSize,
        PhysAddr,
        PhysFrame,
        Size4KiB,
        align_down,
        align_up,
    },
    bootinfo::{get_phys_offset, usable_mmap_entries},
    sync::locks::Mutex,
};

pub type GlobalFrameAllocator = LinkedListFrameAllocator;
pub static GLOBAL_FRAME_ALLOCATOR: OnceCell<Mutex<GlobalFrameAllocator>> = OnceCell::uninit();

pub fn init_frame_alloc() {
    GLOBAL_FRAME_ALLOCATOR.init_once(|| Mutex::new(GlobalFrameAllocator::new()));
}

pub fn get_frame_alloc<'a>() -> &'a Mutex<GlobalFrameAllocator> {
    GLOBAL_FRAME_ALLOCATOR.get().unwrap()
}

pub struct LinkedListFrameAllocator {
    head: *mut u64,
    current_batch_end: usize,
}

impl LinkedListFrameAllocator {
    fn new() -> Self {
        let initial = null_mut();
        let mut alloc = Self {
            head: initial,
            current_batch_end: 0,
        };
        alloc.add_batch();
        alloc
    }

    fn add_batch(&mut self) {
        let usable_regions = usable_mmap_entries();
        let frames = usable_regions
            .map(|r| {
                align_up(r.start, Size4KiB::SIZE)..align_down(r.start + r.length, Size4KiB::SIZE)
            })
            .flat_map(|r| r.step_by(4096))
            .map(|r| PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(r)));

        let next_batch = frames.skip(self.current_batch_end).take(20000); // 20000 * 4KiB = 8MiB
        for frame in next_batch {
            unsafe {
                self.deallocate_frame(frame);
            }
            self.current_batch_end += 1;
        }
    }
}

impl FrameDeallocator<Size4KiB> for LinkedListFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        // write current head into frame and point head to frame

        let addr = (frame.start_address().as_u64() + get_phys_offset()) as *mut u64;
        unsafe { addr.write(self.head as u64) };
        self.head = addr;
    }
}

unsafe impl FrameAllocator<Size4KiB> for LinkedListFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        // get current frame from head and update head to point to next
        if self.head.is_null() {
            self.add_batch();
            if self.head.is_null() {
                // tried to add more frames, but none are available
                return None;
            }
        }

        let next_head = unsafe { *self.head };
        let current_phys = self.head as u64 - get_phys_offset();
        self.head = next_head as *mut u64;

        let frame = PhysFrame::containing_address(PhysAddr::new(current_phys));
        unsafe {
            core::ptr::write_bytes(
                (frame.start_address().as_u64() + get_phys_offset()) as *mut u8,
                0,
                Size4KiB::SIZE as usize,
            );
        }
        Some(frame)
    }
}

unsafe impl Send for LinkedListFrameAllocator {}
