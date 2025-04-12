//TODO

// use x86_64::{
//     PhysAddr,
//     structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
// };

// use crate::bootinfo::UsableMRegion;

// pub struct SimpleFrameAllocator<I>
// where
//     I: Iterator<Item = UsableMRegion>,
// {
//     usable_mmap_entries: I,
// }

// impl<I> SimpleFrameAllocator<I>
// where
//     I: Iterator<Item = UsableMRegion>,
// {
//     pub fn new(entries: I) -> Self {
//         Self {
//             usable_mmap_entries: entries,
//         }
//     }
// }

// unsafe impl<I> FrameAllocator<Size4KiB> for SimpleFrameAllocator<I>
// where
//     I: Iterator<Item = UsableMRegion>,
// {
//     fn allocate_frame(&mut self) -> Option<PhysFrame> {
//         self.usable_mmap_entries
//             .next()
//             .map(|e| PhysFrame::from_start_address(PhysAddr::new(e.start)).unwrap())
//     }
// }

// impl From<UsableMRegion> for PhysFrame {
//     fn from(value: UsableMRegion) -> Self {
//         Self {
//             start_address: PhysAddr::new(value.start),
//             size: core::marker::PhantomData,
//         }
//     }
// }
