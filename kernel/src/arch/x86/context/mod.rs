use core::arch::{asm, global_asm};

use lazy_static::lazy_static;
use x86_64::registers::rflags::RFlags;

use crate::{
    arch::{
        mem::{
            FrameAllocator, FrameDeallocator, Mapper, Page, PageSize, PageTableFlags, Size4KiB,
            VirtAddr,
        },
        x86::interrupt::{
            gdt::get_kernel_selectors,
            handlers::{InterruptStackFrame, interrupt_cleanup},
        },
    },
    kernel::{
        mem::paging::{GLOBAL_FRAME_ALLOCATOR, PAGETABLE, TaskPageTable},
        threading::{
            ThreadingError,
            task::{SimpleTask, Task},
        },
    },
    serial_println,
};
use spin::Mutex;

use super::interrupt::gdt::get_user_selectors;

const KSTACK_AREA_START: VirtAddr = VirtAddr::new(0xffff_8000_c000_0000); // random location
const KSTACK_USER_AREA_START: VirtAddr = VirtAddr::new(0xffff_8000_d000_0000);

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

    pub fn from_trap_ctx(frame: InterruptStackFrame, ctx: ReducedCpuInfo) -> Self {
        Self {
            rsp: frame.stack_pointer.as_u64(),
            rflags: frame.cpu_flags.bits(),
            ss: frame.stack_segment.0 as u64,
            cs: frame.code_segment.0 as u64,
            rip: frame.instruction_pointer.as_u64(),
            r15: ctx.r15,
            r14: ctx.r14,
            r13: ctx.r13,
            r12: ctx.r12,
            r11: ctx.r11,
            r10: ctx.r10,
            r9: ctx.r9,
            r8: ctx.r8,
            rsi: ctx.rsi,
            rbp: ctx.rbp,
            rdi: ctx.rdi,
            rdx: ctx.rdx,
            rcx: ctx.rcx,
            rbx: ctx.rbx,
            cr3: ctx.cr3,
            rax: ctx.rax,
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

// #[derive(Default, Debug)]
// #[repr(C)]
// pub struct ReducedCpuInfo {
//     /// Cpu state not passed via Interrupt frame\
//     cr3: u64,
//     rax: u64,
//     rbx: u64,
//     rcx: u64,
//     rdx: u64,
//     rsi: u64,
//     rdi: u64,

//     r8: u64,
//     r9: u64,
//     r10: u64,
//     r11: u64,
//     r12: u64,
//     r13: u64,
//     r14: u64,
//     r15: u64,
// }

#[derive(Default, Debug)]
#[repr(C)]
pub struct ReducedCpuInfo {
    /// Cpu state not passed via Interrupt frame\
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    cr3: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rax: u64,
}

// alternative: push taskctx to its kernel stack -> switch kernel stack -> pop context -> iretq
global_asm!(
    "
        .global set_cpu_context    
        .global save_reduced_cpu_context
        .global save_context_local
        .global get_context_local
        .global switch_and_apply
        .global init_kernel_task
        .global init_usr_task

        set_cpu_context:
            /// TaskCtx ptr in rdi
            /// installs task, cleans up and returns from trap
            // goto new kernel stack
            // mov rsp, [rdi + x]// need Task info 
            // push interrupt frame
            // ss | rsp | rflags | cs | rip
            push [rdi + 16]
            push rdi
            push [rdi + 8]
            push [rdi + 24]
            push [rdi + 32]

            // set registers
            mov r15, [rdi + 40]
            mov r14, [rdi + 48]
            mov r13, [rdi + 56]
            mov r12, [rdi + 64]
            mov r11, [rdi + 72]
            mov r10, [rdi + 80]
            mov r9, [rdi + 88]
            mov r8, [rdi + 96]

            mov rsi, [rdi + 104]
            mov rbp, [rdi + 112]
            mov rdx, [rdi + 128]
            mov rcx, [rdi + 136]
            mov rbx, [rdi + 144]
            mov rax, [rdi + 152]
            mov cr3, rax
            mov rax, [rdi + 160]
            mov rdi, [rdi + 120]
            
            call interrupt_cleanup
            // unreachable
            ud2

        save_context_local:
            /// stack layout at entry:
            /// <stuff> Interrupt frame
            /// at exit:
            /// <stuff> Interrupt Frame ReducedCpuState
            push rax
            push rbp
            push rdi
            push rsi
            push rdx
            push rcx
            push rbx
            mov rax, cr3
            push rax
            push r15
            push r14
            push r13
            push r12
            push r11
            push r10
            push r9
            push r8
            ret

        get_context_local:
            /// undoes save_context_local
            pop r8
            pop r9
            pop r10
            pop r11
            pop r12
            pop r13
            pop r14
            pop r15
            pop rax
            mov cr3, rax
            pop rbx
            pop rcx
            pop rdx
            pop rsi
            pop rdi
            pop rbp
            pop rax
            ret
              
        //TODO correct
        save_reduced_cpu_context:
            push rax
            lea rax, [rsp + 8]
            push rdi
            push rsi
            push rdx
            push rcx
            push rbx
            mov rax, cr3
            push rax
            push r15
            push r14
            push r13
            push r12
            push r11
            push r10
            push r9
            push r8
            mov rdx, rsp
            ret

        switch_and_apply:
            /// is called from context_switch_local after a trap
            /// needs to:
            /// switch to correct kstack (saved in passed SimpleTask)
            /// apply context saved on this stack
            /// clean up the interrupt
            /// iretq
            mov rsp, [rdi]
            // now on tasks kstack, with state on stack
            pop r8
            pop r9
            pop r10
            pop r11
            pop r12
            pop r13
            pop r14
            pop r15
            pop rax
            mov cr3, rax
            pop rbx
            pop rcx
            pop rdx
            pop rsi
            pop rdi
            pop rbp
            pop rax
            call interrupt_cleanup

        init_kernel_task:
            mov rax, rsp
            mov rsp, [rdi + 8]
            // now on tasks kstack
            // 1: push interrupt frame
            push [rdi + 32] // ss
            push [rdi + 8] // rsp
            push [rdi + 24] // rflags
            push [rdi + 16] // cs
            push [rdi + 0] // rip

            // 2: push Cpu Context, such that it can be popped by switch_and_apply
            push 0 // rax
            push 0 // rbp
            push 0 // rdi
            push 0 // rsi
            push 0 // rdx
            push 0 // rcx
            push 0 // rbx
            mov rsi, cr3
            push rsi // cr3 TODO
            push 0 // r15
            push 0
            push 0
            push 0
            push 0
            push 0
            push 0
            push 0 // r8
            mov rsp, rax
            ret

        init_usr_task:
            ret
    "
);

fn serial_stub__(v1: u64, v2: u64) {
    serial_println!("v1: {:x}, v2: {:x}", v1, v2);
}

unsafe extern "C" {
    pub fn save_reduced_cpu_context() -> (*const InterruptStackFrame, *const ReducedCpuInfo);
    pub fn set_cpu_context(ctx: TaskCtx);
    pub fn save_context_local();
    pub fn get_context_local();
    pub fn switch_and_apply(task: &SimpleTask);
    pub fn init_kernel_task(info: KTaskInfo) -> VirtAddr;
    pub fn init_usr_task(info: UsrTaskInfo);
}

pub struct KTaskInfo {
    rip: VirtAddr,
    kstack_top: VirtAddr,
    cs: u64,
    rflags: u64,
    ss: u64,
}

impl KTaskInfo {
    pub fn new(addr: VirtAddr, kstack: VirtAddr) -> Self {
        serial_println!("rip: {:x}", addr.as_u64());
        let (cs, ss) = get_kernel_selectors();
        let rflags = RFlags::INTERRUPT_FLAG | RFlags::from_bits_truncate(0x2);
        Self {
            rip: addr,
            kstack_top: kstack,
            cs: cs.0 as u64,
            rflags: rflags.bits(),
            ss: ss.0 as u64,
        }
    }
}

pub struct UsrTaskInfo {
    rip: VirtAddr,
    kstack_top: VirtAddr,
    usr_stack_top: VirtAddr,
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

        assert!(mapper.translate_page(start_page).is_err());

        for page in Page::range_inclusive(start_page, end_page) {
            let frame = frame_allocator
                .allocate_frame()
                .ok_or(ThreadingError::StackNotBuilt)?;
            unsafe {
                mapper
                    .map_to(page, frame, flags, &mut *frame_allocator)
                    .map_err(|_| ThreadingError::StackNotBuilt)?
                    // .unwrap()
                    .flush();
            };
        }
        assert!(mapper.translate_page(end_page).is_ok());
    }
    serial_println!("mapped");
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
