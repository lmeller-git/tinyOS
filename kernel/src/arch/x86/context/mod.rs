use core::arch::{asm, global_asm};

use lazy_static::lazy_static;
use x86_64::{registers::rflags::RFlags, structures::paging::FrameDeallocator};

use crate::{
    arch::{
        mem::{FrameAllocator, Mapper, Page, PageSize, PageTableFlags, Size4KiB, VirtAddr},
        x86::interrupt::gdt::get_kernel_selectors,
    },
    kernel::{
        mem::paging::{GLOBAL_FRAME_ALLOCATOR, PAGETABLE, TaskPageTable},
        threading::ThreadingError,
    },
};
use spin::Mutex;

use super::interrupt::gdt::get_user_selectors;

const KSTACK_AREA_START: VirtAddr = VirtAddr::new(0xffff_8000_1000_0000); // random locations
const KSTACK_USER_AREA_START: VirtAddr = VirtAddr::new(0xffff_8000_2000_0000);

const KSTACK_SIZE: usize = 16 * 1024; // 16 KiB
const KSTACK_SIZE_USER: usize = 8 * 1024; // 8 KiB

const MAX_KSTACKS: usize = 512;
const MAX_USER_KSTACKS: usize = 1024;

const USER_STACK_START: VirtAddr = VirtAddr::new(0x0000_0000_1000_0000); // random location
const USER_STACK_SIZE: usize = 1024 * 1024; // 1MiB

lazy_static! {
    static ref KSTACKS_IN_USAGE: Mutex<[bool; MAX_KSTACKS]> = Mutex::new([false; MAX_KSTACKS]);
}

lazy_static! {
    static ref USER_KSTACKS_IN_USAGE: Mutex<[bool; MAX_USER_KSTACKS]> =
        Mutex::new([false; MAX_USER_KSTACKS]);
}

#[derive(Default)]
#[repr(C)]
pub struct TaskCtx {
    pub rsp: u64,
    pub rflags: u64,
    pub ss: u64,
    pub cs: u64,
    pub rip: u64,

    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,

    pub rsi: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub cr3: u64,
    pub rax: u64,
}

impl TaskCtx {
    pub fn new_kernel(entry: usize, stack_top: VirtAddr) -> Self {
        assert!(stack_top.as_u64() % 16 == 0);
        let (cs, ss) = get_kernel_selectors();
        let rflags = RFlags::INTERRUPT_FLAG | RFlags::from_bits_truncate(0x2);
        Self {
            rsp: stack_top.as_u64(),
            rflags: rflags.bits(),
            rip: entry as u64,
            cs: cs.0 as u64,
            ss: ss.0 as u64,
            ..Default::default()
        }
    }

    pub fn new_user(entry: usize, stack_top: VirtAddr) -> Self {
        let (cs, ss) = get_user_selectors();
        let rflags = RFlags::INTERRUPT_FLAG | RFlags::from_bits_truncate(0x2);
        Self {
            rsp: stack_top.as_u64(),
            rflags: rflags.bits(),
            rip: entry as u64,
            cs: cs.0 as u64,
            ss: ss.0 as u64,
            ..Default::default()
        }
    }
    // this does not work, as these will be changed by the time we get here.
    #[inline(always)]
    pub fn store_current(&mut self) {
        // unsafe {
        //     asm!(
        //         "mov {0}, r15",
        //         "mov {1}, r14",
        //         "mov {2}, r13",
        //         "mov {3}, r12",
        //         "mov {4}, r11",
        //         "mov {5}, r10",
        //         "mov {6}, r9",
        //         "mov {7}, r8",
        //         "mov {8}, rbp",
        //         "mov {9}, rdi",
        //         "mov {10}, rdx",
        //         "mov {11}, rcx",
        //         "mov {12}, rbx",
        //         "mov {13}, cr3",
        //         "mov {14}, rax",
        //         "mov {15}, rsi",
        //         out(reg) self.r15,
        //         out(reg) self.r14,
        //         out(reg) self.r13,
        //         out(reg) self.r12,
        //         out(reg) self.r11,
        //         out(reg) self.r10,
        //         out(reg) self.r9,
        //         out(reg) self.r8,
        //         out(reg) self.rbp,
        //         out(reg) self.rdi,
        //         out(reg) self.rdx,
        //         out(reg) self.rcx,
        //         out(reg) self.rbx,
        //         out(reg) self.cr3,
        //         out(reg) self.rax,
        //         out(reg) self.rsi
        //     )
        // }
    }
}

#[derive(Default, Debug)]
#[repr(C)]
pub struct ReducedCpuInfo {
    /// Cpu state not passed via Interrupt frame\
    cr3: u64,
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,

    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

//TODO pass interrupt frame and possbily current_state correctly to context_switch
global_asm!(
    "
    .global context_switch_stub
    context_switch_stub:
        push r15
        push r14
        push r13
        push r12
        push r11
        push r10
        push r9
        push r8

        push rdi
        push rsi
        push rdx
        push rcx
        push rbx
        push rax
        mov rax, cr3
        push rax
        // rsp, rip, rflags, cs, ss in interruptframe
        // sub rsp, 8
        mov rdi, rsp 
        lea rsi, [rsp - 15 * 8]
        call {0}
        // add rsp, 8
    ",
    sym crate::kernel::threading::schedule::context_switch
);

global_asm!(
    "
        .global save_cpu_state
        save_cpu_state:
            /// pushes all relevant registers to the stack and returns a pointer to a ReducedCPUState
            push r15
            push r14
            push r13
            push r12
            push r11
            push r10
            push r9
            push r8

            push rdi
            push rsi
            push rdx
            push rcx
            push rbx
            push rax
            mov rax, cr3
            push rax
            
            mov rax, rsp
            ret
    "
);

unsafe extern "C" {
    pub fn save_cpu_state() -> *const ReducedCpuInfo;
}

pub fn allocate_kstack() -> Result<VirtAddr, ThreadingError> {
    // let (current_table, current_flags) = current_page_tbl();
    let flags = PageTableFlags::WRITABLE | PageTableFlags::PRESENT;

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

    let base =
        (KSTACK_AREA_START + kstack_start_idx as u64 * KSTACK_SIZE as u64).align_up(Size4KiB::SIZE);
    let start = base + Size4KiB::SIZE;
    let end = base + KSTACK_SIZE as u64 - 1;

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
    Ok(end)
}

pub fn free_kstack(top: VirtAddr) -> Result<(), ThreadingError> {
    // assuming top is a properly aligned addr in the correct region
    let start = top + 1 - KSTACK_SIZE as u64;
    let idx = (start - KSTACK_AREA_START) as usize / KSTACK_SIZE;

    let start_page = Page::containing_address(start + Size4KiB::SIZE);
    let end_page = Page::containing_address(top);
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
    *KSTACKS_IN_USAGE
        .lock()
        .get_mut(idx)
        .ok_or(ThreadingError::StackNotFreed)? = false;

    Ok(())
}

pub fn allocate_userstack(tbl: &mut TaskPageTable) -> Result<VirtAddr, ThreadingError> {
    let flags = PageTableFlags::WRITABLE | PageTableFlags::PRESENT;

    let base = USER_STACK_START.align_up(Size4KiB::SIZE);
    let start = base + Size4KiB::SIZE;
    let end = base + USER_STACK_SIZE as u64 - 1;

    let start_page = Page::containing_address(start);
    let end_page = Page::containing_address(end);

    {
        let mapper = &mut tbl.table;
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
            }
        }
    }

    Ok(end)
}

pub fn allocate_userkstack(tbl: &mut TaskPageTable) -> Result<VirtAddr, ThreadingError> {
    let flags = PageTableFlags::WRITABLE | PageTableFlags::PRESENT;

    let kstack_start_idx = {
        let mut in_use = USER_KSTACKS_IN_USAGE.lock();
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

    let base = (KSTACK_USER_AREA_START + kstack_start_idx as u64 * KSTACK_SIZE_USER as u64)
        .align_up(Size4KiB::SIZE);
    let start = base + Size4KiB::SIZE;
    let end = base + KSTACK_SIZE_USER as u64 - 1;

    let start_page = Page::containing_address(start);
    let end_page = Page::containing_address(end);

    {
        let mapper = &mut tbl.table;
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
    Ok(end)
}

pub fn free_user_kstack(top: VirtAddr, tbl: &mut TaskPageTable) -> Result<(), ThreadingError> {
    // assuming top is a properly aligned addr in the correct region
    let start = top + 1 - KSTACK_SIZE_USER as u64;
    let idx = (start - KSTACK_USER_AREA_START) as usize / KSTACK_SIZE_USER;

    let start_page = Page::containing_address(start + Size4KiB::SIZE);
    let end_page = Page::containing_address(top);
    {
        let mapper = &mut tbl.table;
        let mut frame_allocator = GLOBAL_FRAME_ALLOCATOR.lock();
        for page in Page::range_inclusive(start_page, end_page) {
            let (frame, flush) = mapper
                .unmap(page)
                .map_err(|_| ThreadingError::StackNotFreed)?;
            flush.flush();
            unsafe { frame_allocator.deallocate_frame(frame) };
        }
    }

    *USER_KSTACKS_IN_USAGE
        .lock()
        .get_mut(idx)
        .ok_or(ThreadingError::StackNotFreed)? = false;

    Ok(())
}

pub fn free_user_stack(top: VirtAddr, tbl: &mut TaskPageTable) -> Result<(), ThreadingError> {
    // assuming top is a properly aligned addr in the correct region
    let start = top + 1 - KSTACK_SIZE_USER as u64;

    let start_page = Page::containing_address(start + Size4KiB::SIZE);
    let end_page = Page::containing_address(top);
    {
        let mapper = &mut tbl.table;
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
