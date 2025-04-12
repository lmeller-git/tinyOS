//TODO

use crate::bootinfo::usable_mmap_entries;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{
    PhysAddr,
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
};

// Does not free pages
pub struct SimpleFrameAllocator {
    current: usize,
}

impl SimpleFrameAllocator {
    fn new() -> Self {
        Self { current: 0 }
    }

    // pub fn allocate_frames(&mut self, n: usize) -> &[Option<PhysFrame<Size4KiB>>] {
    //     let frames = self.usable_frames();
    //     let mut container: [Option<PhysFrame>; n] = [None];
    // }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let usable_regions = usable_mmap_entries();
        usable_regions
            .map(|r| r.start..r.start + r.length)
            .flat_map(|r| r.step_by(4096))
            .map(|r| PhysFrame::containing_address(PhysAddr::new(r)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for SimpleFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let r = self.usable_frames().nth(self.current);
        self.current += 1;
        r
    }
}

// fn init_allocator() -> SimpleFrameAllocator {
//     let usable_regions = usable_mmap_entries();
//     let ranges = usable_regions
//         .map(|r| r.start..r.start + r.length)
//         .flat_map(|r| r.step_by(4096))
//         .map(|r| PhysFrame::containing_address(PhysAddr::new(r)));
//     SimpleFrameAllocator::new()
// }

// lazy_static!{
//     static ref FRAME_MAP:
// }

lazy_static! {
    // pub static ref GlobalFrameAllocator: Mutex<SimpleFrameAllocator> = Mutex::new(init_allocator());
    pub static ref GlogalFrameAllocator: Mutex<SimpleFrameAllocator> = Mutex::new(SimpleFrameAllocator::new());
}
