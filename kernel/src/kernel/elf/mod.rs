use elf::endian::AnyEndian;
use x86_64::structures::paging::Translate;

use crate::{
    arch::{
        interrupt,
        mem::{
            Cr3,
            Cr3Flags,
            FrameAllocator,
            Mapper,
            OffsetPageTable,
            Page,
            PageSize,
            PageTableFlags,
            PhysFrame,
            Size4KiB,
            VirtAddr,
        },
    },
    kernel::{
        mem::paging::{
            APageTable,
            PAGETABLE,
            TaskPageTable,
            get_frame_alloc,
            get_kernel_pagetbl_root,
        },
        threading::{task::TaskRepr, tls},
    },
    serial_println,
};

pub fn apply<M1: Mapper<Size4KiB>>(
    bytes: &elf::ElfBytes<AnyEndian>,
    data: &[u8],
    table: &mut M1,
) -> Result<(), ElfError> {
    serial_println!("writing elf data into memory...");
    let headers = bytes.segments().ok_or(ElfError::Unknown)?;
    for header in headers.iter() {
        let addr = VirtAddr::new(header.p_vaddr);
        let mapper = PageMapper::init(&addr, header.p_memsz);
        let active_table_root: PhysFrame<Size4KiB> = if let Some(current) =
            tls::task_data().current_thread()
            && let Some(task_tbl) = current.pagedir().try_get_owned()
        {
            task_tbl.lock().root
        } else {
            get_kernel_pagetbl_root().clone()
        };

        let global_table = &mut *PAGETABLE.lock();
        // lock frame alloc to ensure we do not deadlock during interrupt disabled context
        let _alloc = get_frame_alloc().lock();

        // SAFETY: This is safe, if we can ensure that interrupts will be restored upon ret
        // This is the case, even if we panic
        unsafe {
            interrupt::disable();
            Cr3::write(get_kernel_pagetbl_root().clone(), Cr3Flags::empty());
        }
        drop(_alloc);

        mapper.map(table, get_pagetableflags(header.p_flags), global_table);
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
        mapper.unmap(global_table);

        unsafe {
            Cr3::write(active_table_root, Cr3Flags::empty());
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

    fn map<M1: Mapper<Size4KiB>, M2: Mapper<Size4KiB>>(
        &self,
        new: &mut M1,
        flags: PageTableFlags,
        old: &mut M2,
    ) {
        let mut alloc = get_frame_alloc().lock();
        for page in Page::range_inclusive(self.start, self.end) {
            if new.translate_page(page).is_ok() {
                continue;
            }
            let frame = alloc.allocate_frame().unwrap();
            unsafe {
                _ = new
                    .map_to(page, frame, flags, &mut *alloc)
                    .map(|f| f.flush());
                _ = old
                    .map_to(page, frame, flags | PageTableFlags::WRITABLE, &mut *alloc)
                    .map(|f| f.flush());
            }
        }
    }

    fn unmap<M1: Mapper<Size4KiB>>(&self, table: &mut M1) {
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
