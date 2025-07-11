use core::ptr::null;

use crate::{
    arch::{
        interrupt,
        mem::{
            Cr3, Cr3Flags, FrameAllocator, Mapper, Page, PageSize, PageTableFlags, Size4KiB,
            VirtAddr,
        },
    },
    kernel::mem::paging::{GLOBAL_FRAME_ALLOCATOR, TaskPageTable},
};
use elf::endian::AnyEndian;

pub fn parse(bytes: &[u8]) -> Result<VirtAddr, elf::ParseError> {
    let mut parser = elf::ElfBytes::<AnyEndian>::minimal_parse(bytes)?;
    let headers = parser.segments().unwrap();
    for header in headers.iter() {
        let offset = header.p_offset;
        let virt = header.p_vaddr;
        let file_size = header.p_filesz;
        let mem_size = header.p_memsz;
        let flags = header.p_flags;
        let align = header.p_align;

        // let data = &bytes[offset..offset + file_size];
        // now copy this to mem and init pagedir
    }
    let entry = parser.ehdr.e_entry;
    Ok(VirtAddr::new(entry))
}

pub fn apply(
    bytes: &elf::ElfBytes<AnyEndian>,
    data: &[u8],
    table: &mut TaskPageTable,
) -> Result<(), ElfError> {
    let headers = bytes.segments().ok_or(ElfError::Unknown)?;
    for header in headers.iter() {
        let addr = VirtAddr::new(header.p_vaddr);
        map_pages(
            table,
            &addr,
            header.p_filesz,
            get_pagetableflags(header.p_flags),
        );
        //TODO: load tbl into cr3 and reload old cr3 afterwards
        let (current, flags) = Cr3::read();
        // SAFETY: This is safe, if we can ensure that Cr3 will be restored upon ret and interrupts are enabled again
        unsafe {
            interrupt::disable();
            Cr3::write(table.root, Cr3Flags::empty());
        }
        copy_to_mem(
            &addr,
            &data[header.p_offset as usize..header.p_offset as usize + header.p_filesz as usize],
        );

        if header.p_memsz > header.p_filesz {
            zero_mem(
                &(addr + header.p_filesz),
                (header.p_memsz - header.p_filesz) as usize,
            );
        }

        unsafe {
            Cr3::write(current, flags);
            interrupt::enable();
        }
    }
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

fn map_pages(table: &mut TaskPageTable, addr: &VirtAddr, size: u64, flags: PageTableFlags) {
    let start = Page::containing_address(addr.align_down(Size4KiB::SIZE));
    let end = Page::containing_address((*addr + size - 1).align_down(Size4KiB::SIZE));
    let mut alloc = GLOBAL_FRAME_ALLOCATOR.lock();
    for page in Page::range_inclusive(start, end) {
        let frame = alloc.allocate_frame().unwrap();
        unsafe {
            table
                .table
                .map_to(page, frame, flags, &mut *alloc)
                .unwrap()
                .flush();
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
