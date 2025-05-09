use core::ptr::NonNull;

use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, Page, PageSize, PageTableFlags, PhysFrame, Size4KiB,
    },
};

#[derive(Clone)]
struct Foo {}

impl acpi::AcpiHandler for Foo {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let phys_start = PhysAddr::new(physical_address as u64);
        let virt_start =
            VirtAddr::new(physical_address as u64 + crate::bootinfo::get_phys_offset());

        let start_page: Page<Size4KiB> = Page::containing_address(virt_start);
        let end_page: Page<Size4KiB> = Page::containing_address(virt_start + size as u64 - 1);

        let mut mapper = crate::kernel::mem::paging::PAGETABLE.lock();
        let mut frame_allocator = crate::kernel::mem::paging::GLOBAL_FRAME_ALLOCATOR.lock();

        for page in Page::range_inclusive(start_page, end_page) {
            let frame = PhysFrame::containing_address(PhysAddr::new(
                page.start_address().as_u64() - crate::bootinfo::get_phys_offset(),
            ));
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
            unsafe {
                mapper
                    .map_to(page, frame, flags, &mut *frame_allocator)
                    .unwrap()
                    .flush();
            }
        }

        unsafe {
            acpi::PhysicalMapping::new(
                physical_address,
                NonNull::new(virt_start.as_mut_ptr()).unwrap(),
                size,
                size,
                self.clone(),
            )
        }
    }

    fn unmap_physical_region<T>(region: &acpi::PhysicalMapping<Self, T>) {
        // TODO
        crate::kernel::mem::paging::PAGETABLE
            .lock()
            .unmap(
                Page::<Size4KiB>::from_start_address(VirtAddr::new(
                    region.virtual_start().addr().get() as u64,
                ))
                .unwrap(),
            )
            .unwrap()
            .1
            .flush();
    }
}

pub(super) fn apic_init() {
    let handler =
        unsafe { acpi::AcpiTables::from_rsdp(Foo {}, crate::bootinfo::rdsp_addr()).unwrap() };
    let table = handler.find_table::<acpi::madt::Madt>().unwrap();
    // table.
    handler.revision();
    handler.headers();
}
