use core::ptr::null;

use crate::{
    arch::{
        interrupt,
        mem::{
            Cr3, Cr3Flags, FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTableFlags,
            Size4KiB, VirtAddr,
        },
    },
    kernel::mem::paging::{PAGETABLE, TaskPageTable, get_frame_alloc},
    serial_println,
};
use elf::endian::AnyEndian;

pub fn apply(
    bytes: &elf::ElfBytes<AnyEndian>,
    data: &[u8],
    table: &mut TaskPageTable,
) -> Result<(), ElfError> {
    let headers = bytes.segments().ok_or(ElfError::Unknown)?;
    for header in headers.iter() {
        serial_println!("getting addr");
        let addr = VirtAddr::new(header.p_vaddr);
        let mapper = PageMapper::init(&addr, header.p_filesz);
        let mut global_table = PAGETABLE.lock();

        // SAFETY: This is safe, if we can ensure that interrupts will be restored upon ret
        // unsafe {
        //     interrupt::disable();
        // }
        mapper.map(table, get_pagetableflags(header.p_flags), &mut global_table);
        copy_to_mem(
            &addr,
            &data[header.p_offset as usize..header.p_offset as usize + header.p_filesz as usize],
        );
        serial_println!("copied");

        if header.p_memsz > header.p_filesz {
            zero_mem(
                &(addr + header.p_filesz),
                (header.p_memsz - header.p_filesz) as usize,
            );
        }
        serial_println!("zeroed");
        mapper.unmap(&mut global_table);

        // unsafe {
        //     interrupt::enable();
        // }
    }
    serial_println!("done");
    Ok(())
}

fn get_pagetableflags(elf_flags: u32) -> PageTableFlags {
    let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    if elf_flags & elf::abi::PF_W != 0 {
        flags |= PageTableFlags::WRITABLE;
    }

    if elf_flags & elf::abi::PF_X == 0 {
        flags |= PageTableFlags::NO_EXECUTE;
    }

    flags
}

struct PageMapper {
    start: Page<Size4KiB>,
    end: Page<Size4KiB>,
}

impl PageMapper {
    fn init(addr: &VirtAddr, size: u64) -> Self {
        Self {
            start: Page::containing_address(addr.align_down(Size4KiB::SIZE)),
            end: Page::containing_address((*addr + size - 1).align_down(Size4KiB::SIZE)),
        }
    }

    fn map(&self, new: &mut TaskPageTable, flags: PageTableFlags, old: &mut OffsetPageTable<'_>) {
        let mut alloc = get_frame_alloc().lock();
        for page in Page::range_inclusive(self.start, self.end) {
            // if new.table.translate_page(page).is_ok() {
            //     continue;
            // }
            let frame = alloc.allocate_frame().unwrap();
            unsafe {
                _ = new
                    .table
                    .map_to(page, frame, flags, &mut *alloc)
                    .map(|f| f.flush());
                _ = old
                    .map_to(page, frame, flags | PageTableFlags::WRITABLE, &mut *alloc)
                    .map(|f| f.flush());
            }
        }
    }

    fn unmap(&self, table: &mut OffsetPageTable) {
        for page in Page::range_inclusive(self.start, self.end) {
            table.unmap(page).unwrap().1.flush();
        }
    }
}

fn copy_to_mem(addr: &VirtAddr, data: &[u8]) {
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), addr.as_mut_ptr(), data.len());
    }
}

fn zero_mem(start: &VirtAddr, len: usize) {
    unsafe {
        core::ptr::write_bytes(start.as_mut_ptr::<u8>(), 0, len);
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ElfError {
    Unknown,
}
