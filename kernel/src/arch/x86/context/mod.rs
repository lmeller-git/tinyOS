use super::interrupt::gdt::get_user_selectors;
use crate::{
    arch::{
        mem::{
            FrameAllocator, FrameDeallocator, Mapper, Page, PageSize, PageTableFlags, Size4KiB,
            VirtAddr,
        },
        x86::{
            interrupt::{gdt::get_kernel_selectors, handlers::InterruptStackFrame},
            mem::PhysAddr,
        },
    },
    kernel::{
        mem::paging::{GLOBAL_FRAME_ALLOCATOR, PAGETABLE, TaskPageTable},
        threading::{ThreadingError, task::SimpleTask},
    },
    serial_println,
};
use core::arch::global_asm;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::registers::rflags::RFlags;

const KSTACK_AREA_START: VirtAddr = VirtAddr::new(0xffff_f000_c000_0000); // random location
const KSTACK_USER_AREA_START: VirtAddr = VirtAddr::new(0xffff_f000_f000_0000);

const KSTACK_SIZE: usize = 16 * 1024; // 16 KiB
const KSTACK_SIZE_USER: usize = 8 * 1024; // 8 KiB

const MAX_KSTACKS: usize = 512; // random num (this is also max kernel tasks)
const MAX_USER_KSTACKS: usize = 1024; // random num (this is also max user tasks)

const USER_STACK_START: VirtAddr = VirtAddr::new(0x0000_0000_1000_0000); // random location
const USER_STACK_SIZE: usize = 1024 * 1024; // 1MiB

lazy_static! {
    static ref KSTACKS_IN_USAGE: Mutex<[bool; MAX_KSTACKS]> = Mutex::new([false; MAX_KSTACKS]);
}

lazy_static! {
    static ref USER_KSTACKS_IN_USAGE: Mutex<[bool; MAX_USER_KSTACKS]> =
        Mutex::new([false; MAX_USER_KSTACKS]);
}

//TODO
#[derive(Default, Debug)]
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
        todo!()
    }
}

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
        .global switch_and_apply
        .global init_kernel_task
        .global init_usr_task

        switch_and_apply:
            /// is called from context_switch_local after a trap
            /// needs to:
            /// switch to correct kstack (saved in passed SimpleTask)
            /// apply context saved on this stack
            /// clean up the interrupt
            /// iretq
            // mov rsi, rsp // was saved previously
            mov rsp, [rdi]
            // now on tasks kstack, with state on stack
            // push rdi
            // mov rdi, [rdi]
            // call {0}
            // pop rdi

            // mov rsi, rsp
            // call {0}
            
            pop r8
            pop r9
            pop r10
            pop r11
            pop r12
            pop r13
            pop r14
            pop r15
            pop rax // cr3
            mov cr3, rax // indefinite hang??
            pop rbx
            pop rcx
            pop rdx
            pop rsi
            pop rdi
            pop rbp
            pop rax

            // mov rsi, rsp
            // call {0}
            
            jmp interrupt_cleanup

        init_kernel_task:
            mov rax, rsp
            
            // mov rsi, [rdi + 8]
            // call {0} // stack top
            
            mov rsp, [rdi + 8]

            // now on tasks kstack
            // 1: push interrupt frame
            push [rdi + 32] // ss
            push [rdi + 8] // rsp
            push [rdi + 24] // rflags
            push [rdi + 16] // cs
            push [rdi] // rip

            // mov rsi, rsp
            // call {0}

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
            mov rsi, rsp
            mov rsp, rax
            mov rax, rsi

            // mov rsi, rax
            // call {0}
            ret

        init_usr_task:
            mov rax, rsp
            
            // mov rsi, [rdi + 8]
            // call {0} // stack top
            
            mov rsp, [rdi + 8] // kstack top

            // now on tasks kstack
            // 1: push interrupt frame
            push [rdi + 40] // ss
            push [rdi + 16] // usr stack rsp
            push [rdi + 32] // rflags
            push [rdi + 24] // cs
            push [rdi] // rip

            // mov rsi, rsp
            // call {0}

            // 2: push Cpu Context, such that it can be popped by switch_and_apply
            push 0 // rax
            push 0 // rbp
            push 0 // rdi
            push 0 // rsi
            push 0 // rdx
            push 0 // rcx
            push 0 // rbx
            push [rdi + 48] // cr3
            push 0 // r15
            push 0
            push 0
            push 0
            push 0
            push 0
            push 0
            push 0 // r8
            mov rsi, rsp
            mov rsp, rax
            mov rax, rsi
            ret
    ",
    sym serial_stub__
);

pub fn serial_stub__(v1: u64, v2: u64) {
    serial_println!("rsp: {:#x}", v1);
}

unsafe extern "C" {
    pub fn switch_and_apply(task: &SimpleTask);
    pub fn init_kernel_task(info: &KTaskInfo) -> VirtAddr;
    pub fn init_usr_task(info: &UsrTaskInfo) -> VirtAddr;
}

#[repr(C)]
#[derive(Debug)]
pub struct KTaskInfo {
    rip: VirtAddr,
    kstack_top: VirtAddr,
    cs: u64,
    rflags: u64,
    ss: u64,
}

impl KTaskInfo {
    pub fn new(addr: VirtAddr, kstack: VirtAddr) -> Self {
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

#[repr(C)]
#[derive(Debug)]
pub struct UsrTaskInfo {
    rip: VirtAddr,
    kstack_top: VirtAddr,
    usr_stack_top: VirtAddr,
    cs: u64,
    rflags: u64,
    ss: u64,
    cr3: PhysAddr,
}

impl UsrTaskInfo {
    pub fn new(addr: VirtAddr, kstack: VirtAddr, usr: VirtAddr, tbl: PhysAddr) -> Self {
        let (cs, ss) = get_user_selectors();
        //TODO
        let rflags = RFlags::INTERRUPT_FLAG | RFlags::from_bits_truncate(0x2);
        Self {
            rip: addr,
            kstack_top: kstack,
            usr_stack_top: usr,
            cs: cs.0 as u64,
            rflags: rflags.bits(),
            ss: ss.0 as u64,
            cr3: tbl,
        }
    }
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
    let start = base + Size4KiB::SIZE; //.align_up(Size4KiB::SIZE);
    let end = base + KSTACK_SIZE as u64; //.align_up(Size4KiB::SIZE);

    let start_page = Page::containing_address(start);
    let end_page = Page::containing_address(end - 1);

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
                    .flush();
            };
        }
        assert!(mapper.translate_page(end_page).is_ok());
    }
    let stack_top = VirtAddr::new(end.as_u64() - 8); // & !0xF);
    Ok(stack_top)
}

pub fn free_kstack(top: VirtAddr) -> Result<(), ThreadingError> {
    //TODO
    // assuming top is a properly aligned addr in the correct region
    let start = (top + 1 - KSTACK_SIZE as u64).align_up(Size4KiB::SIZE);
    let idx = (start - KSTACK_AREA_START) as usize / KSTACK_SIZE;

    let start_page = Page::containing_address((start + Size4KiB::SIZE).align_up(Size4KiB::SIZE));
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
    // all at the same virt addr
    let flags =
        PageTableFlags::WRITABLE | PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    let base = USER_STACK_START.align_up(Size4KiB::SIZE);
    let start = base + Size4KiB::SIZE; //.align_up(Size4KiB::SIZE);
    let end = base + USER_STACK_SIZE as u64; //.align_up(Size4KiB::SIZE);

    let start_page = Page::containing_address(start);
    let end_page = Page::containing_address(end - 1);

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

    let stack_top = VirtAddr::new(end.as_u64() - 8); // & !0xF);
    Ok(stack_top)
}

pub fn allocate_userkstack(tbl: &mut TaskPageTable) -> Result<VirtAddr, ThreadingError> {
    let flags = PageTableFlags::WRITABLE | PageTableFlags::PRESENT | PageTableFlags::NO_EXECUTE;

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
    let start = base + Size4KiB::SIZE; //.align_up(Size4KiB::SIZE);
    let end = base + KSTACK_SIZE_USER as u64; //.align_up(Size4KiB::SIZE);

    let start_page = Page::containing_address(start);
    let end_page = Page::containing_address(end - 1);

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

    let stack_top = VirtAddr::new(end.as_u64() - 8); // & !0xF);
    Ok(stack_top)
}

pub fn free_user_kstack(top: VirtAddr, tbl: &mut TaskPageTable) -> Result<(), ThreadingError> {
    //TODO
    // assuming top is a properly aligned addr in the correct region
    let start = (top + 1 - KSTACK_SIZE_USER as u64).align_up(Size4KiB::SIZE);
    let idx = (start - KSTACK_USER_AREA_START) as usize / KSTACK_SIZE_USER;

    let start_page = Page::containing_address((start + Size4KiB::SIZE).align_up(Size4KiB::SIZE));
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
    //TODO
    // assuming top is a properly aligned addr in the correct region
    let start = (top + 1 - KSTACK_SIZE_USER as u64).align_up(Size4KiB::SIZE);

    let start_page = Page::containing_address((start + Size4KiB::SIZE).align_up(Size4KiB::SIZE));
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
