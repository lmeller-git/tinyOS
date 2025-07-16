//TODO

use crate::utils::locks::reentrant::Mutex;
use crate::{
    arch::mem::{FrameAllocator, FrameDeallocator, PhysAddr, PhysFrame, Size4KiB},
    bootinfo::usable_mmap_entries,
};
use lazy_static::lazy_static;

// Does not free pages
#[derive(Debug)]
pub struct SimpleFrameAllocator {
    current: usize,
}

impl SimpleFrameAllocator {
    fn new() -> Self {
        Self { current: 0 }
    }

    // pub fn allocate_frames(
    //     &mut self,
    //     mut n: usize,
    // ) -> impl Iterator<Item = PhysFrame<Size4KiB>> + use<'_> {
    //     let mut frames = self.usable_frames();
    //     frames.take_while(|_| {
    //         if n > 0 {
    //             self.current += 1;
    //             n -= 1;
    //             true
    //         } else {
    //             false
    //         }
    //     })
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

impl FrameDeallocator<Size4KiB> for SimpleFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        //TODO
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
    pub static ref  GLOBAL_FRAME_ALLOCATOR: Mutex<SimpleFrameAllocator> = Mutex::new(SimpleFrameAllocator::new());
}
