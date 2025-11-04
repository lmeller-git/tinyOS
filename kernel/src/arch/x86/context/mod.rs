use core::arch::global_asm;

use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{registers::rflags::RFlags, structures::paging::OffsetPageTable};

use super::interrupt::gdt::get_user_selectors;
use crate::{
    arch::{
        mem::{Mapper, Page, PageSize, PageTableFlags, Size4KiB, VirtAddr},
        x86::{
            interrupt::{gdt::get_kernel_selectors, handlers::InterruptStackFrame},
            mem::PhysAddr,
        },
    },
    kernel::{
        mem::paging::{
            PAGETABLE,
            TaskPageTable,
            get_frame_alloc,
            get_kernel_pagetbl_root,
            map_region,
            unmap_region,
        },
        threading::{
            ThreadingError,
            task::{TaskData, TaskRepr},
            trampoline::TaskExitInfo,
        },
    },
};

const KSTACK_AREA_START: VirtAddr = VirtAddr::new(0xffff_f000_c000_0000); // random location

const KSTACK_SIZE: usize = 64 * 1024; // 64 KiB //TODO maybe make this dynamic

const MAX_KSTACKS: usize = 512; // random num (this is also max tasks)

const USER_STACK_START: VirtAddr = VirtAddr::new(0x0000_0000_1000_0000); // random location
const USER_STACK_SIZE: usize = 1024 * 1000; // 1MiB

lazy_static! {
    static ref KSTACKS_IN_USAGE: Mutex<[bool; MAX_KSTACKS]> = Mutex::new([false; MAX_KSTACKS]);
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SysCallCtx {
    pub rax: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,

    pub rbx: u64,
    pub rcx: u64,
    pub r11: u64,

    pub rbp: u64,
}

impl SysCallCtx {
    pub fn ret(&mut self, val: i64) {
        self.rax = val as u64
    }

    pub fn num(&self) -> u64 {
        self.rax
    }

    pub fn first(&self) -> u64 {
        self.rdi
    }

    pub fn second(&self) -> u64 {
        self.rsi
    }

    pub fn third(&self) -> u64 {
        self.rdx
    }

    pub fn fourth(&self) -> u64 {
        self.r10
    }

    pub fn fifth(&self) -> u64 {
        self.r9
    }

    pub fn sixth(&self) -> u64 {
        self.r8
    }

    pub fn ret2(&mut self, val: i64) {
        self.rdx = val as u64
    }
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

/*
callee saved registers:
    rbp
    (rsp)
    rbx
    r15
    r14
    r13
    r12

return:
    rax
    rdx

call:
    rdi
    rsi
    rdx
    rcx
    r8
    r9

temp:
    rax
    r10
    r11
*/

// alternative: push taskctx to its kernel stack -> switch kernel stack -> pop context -> iretq
global_asm!(
    "
        .global switch_and_apply
        .global init_kernel_task
        .global init_usr_task
        .global return_trampoline_stub

        switch_and_apply:
            /// is called from context_switch_local after a trap
            /// needs to:
            /// switch to correct kstack (saved in passed SimpleTask)
            /// apply context saved on this stack
            /// clean up the interrupt
            /// iretq

            mov rsp, rdi

            call end_interrupt
            // now on tasks kstack, with state on stack

            // pop xmm registers
            pop r9
            fxrstor [rsp]
            mov rsp, r9

            pop r8
            pop r9
            pop r10
            pop r11
            pop r12
            pop r13
            pop r14
            pop r15
            pop rax // cr3
            mov cr3, rax
            pop rbx
            pop rcx
            pop rdx
            pop rsi
            pop rdi
            pop rbp
            pop rax

            jmp interrupt_cleanup

        init_kernel_task:
            mov rax, rsp
            mov rsp, [rdi + 8]


            /// pushes return addr after trampoline
            /// trampoline addr
            /// and relevant context
            /// info in rsi

            // ensure alignemnent
            and rsp, -16
            push 0

            push [rsi] // trampoline
            push rsi // task exit info


            // return stub
            lea r8, return_trampoline_stub
            push r8

            // now on tasks kstack
            // 1: push interrupt frame
            mov r8, rsp
            push [rdi + 32] // ss
            push r8  // rsp before ss
            push [rdi + 24] // rflags
            push [rdi + 16] // cs
            push [rdi] // rip

            // 2: push Cpu Context, such that it can be popped by switch_and_apply
            push 0 // rax
            push 0 // rbp
            push [rdx + 0] // rdi
            push [rdx + 8] // rsi
            push [rdx + 16] // rdx
            push [rdx + 24] // rcx
            push 0 // rbx
            mov rsi, cr3
            push rsi // cr3 // we should push the root addr saved in Ktaskinfo, but this triggers a triple fault???
            // push [rdx + 40] // cr3
            push 0 // r15
            push 0
            push 0
            push 0
            push 0
            push 0
            push [rdx + 40]
            push [rdx + 32] // r8

            // save xmm registers
            mov r9, rsp
            sub rsp, 512 + 16
            and rsp, -16
            fxsave [rsp]
            push r9

            // restore rsp
            mov rsi, rsp
            mov rsp, rax
            mov rax, rsi
            ret

        init_usr_task:
            mov rax, rsp
            mov rsp, [rdi + 48] // kernel stack top


            /// pushes return addr after trampoline
            /// trampoline addr
            /// and relevant context
            /// info in rsi

            // ensure alignemnt
            and rsp, -16
            push 0

            push [rsi] // trampoline
            push rsi // task exit info


            call setup_usr_stack
            // user task rsp in r8

            // return stub
            lea r9, return_trampoline_stub
            push r9

            // now on tasks kstack
            // 1: push interrupt frame
            // user variables
            push [rdi + 32] // ss
            push r8
            push [rdi + 24] // rflags
            push [rdi + 16] // cs
            push [rdi]  // rip

            // 2: push Cpu Context, such that it can be popped by switch_and_apply
            push 0 // rax
            push 0 // rbp
            push [rdx + 0] // rdi
            push [rdx + 8] // rsi
            push [rdx + 16] // rdx
            push [rdx + 24] // rcx
            push 0 // rbx
            push [rdi + 40] // cr3
            push 0 // r15
            push 0
            push 0
            push 0
            push 0
            push 0
            push [rdx + 40]
            push [rdx + 32] // r8

            // save xmm registers
            mov r9, rsp
            sub rsp, 512 + 16
            and rsp, -16
            fxsave [rsp]
            push r9

            // restore rsp
            mov rsi, rsp
            mov rsp, rax
            mov rax, rsi
            ret

        setup_usr_stack:
            // usr task info in rdi
            // puts usr task rsp in r8
            mov r9, rsp
            mov rsp, [rdi + 8]
            // now on user stack

            // ensure alignemnt
            and rsp, -16
            sub rsp, 8

            // push return trampolines
            // TODO (currently exit() is expected)

            mov r8, rsp
            mov rsp, r9
            ret

        usr_return_trampoline:
            // on user stack
            // reads rsp of kernel stack, moves execution to it and calls return_trampoline_stub
            // pop rdi
            // mov rsp, rdi
            // simply calls sys_exit via syscall
            mov rax, 1
            mov rdi, 0
            int 0x80
            // we will never reach this point. The remainder is legacy
            ret

        return_trampoline_stub:
            // set up for kernel return trampoline
            // still on task stack
            // return val in rax

            pop rsi // exit info

            mov rdi, rax
            ret // go to trampoline
   ",
);

unsafe extern "C" {
    pub fn switch_and_apply(task: TaskState);
    pub fn init_kernel_task(
        info: &KTaskInfo,
        exit_info: &TaskExitInfo,
        data: &TaskData,
    ) -> VirtAddr;
    pub fn init_usr_task(info: &UsrTaskInfo, exit_info: &TaskExitInfo, data: &TaskData)
    -> VirtAddr;
    pub fn return_trampoline_stub();
}

#[repr(C)]
pub struct TaskState {
    pub rsp: u64,
}

impl TaskState {
    pub fn from_task<T: TaskRepr>(task: &T) -> Self {
        Self {
            rsp: task.krsp().as_u64(),
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct KTaskInfo {
    rip: VirtAddr,
    kstack_top: VirtAddr,
    cs: u64,
    rflags: u64,
    ss: u64,
    cr3: PhysAddr,
}

impl KTaskInfo {
    pub fn new(addr: VirtAddr, kstack: VirtAddr) -> Self {
        let (cs, ss) = get_kernel_selectors();
        let rflags = RFlags::INTERRUPT_FLAG | RFlags::from_bits_truncate(0x2);
        let tbl = get_kernel_pagetbl_root().start_address();
        Self {
            rip: addr,
            kstack_top: kstack,
            cs: cs.0 as u64,
            rflags: rflags.bits(),
            ss: ss.0 as u64,
            cr3: tbl,
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct UsrTaskInfo {
    // user data
    rip: VirtAddr,
    pub usr_stack_top: VirtAddr,
    u_cs: u64,
    u_rflags: u64,
    u_ss: u64,
    pub cr3: PhysAddr,
    // kernel stack data
    pub kstack_top: VirtAddr,
    k_cs: u64,
    k_rflags: u64,
    k_ss: u64,
}

impl UsrTaskInfo {
    pub fn new(addr: VirtAddr, kstack: VirtAddr, usr: VirtAddr, tbl: PhysAddr) -> Self {
        let (cs, ss) = get_user_selectors();
        let (kcs, kss) = get_kernel_selectors();
        //TODO
        let rflags = RFlags::INTERRUPT_FLAG | RFlags::from_bits_truncate(0x2);
        let k_rflags = RFlags::INTERRUPT_FLAG | RFlags::from_bits_truncate(0x2);
        Self {
            rip: addr,
            usr_stack_top: usr,
            u_cs: cs.0 as u64,
            u_rflags: rflags.bits(),
            u_ss: ss.0 as u64,
            cr3: tbl,

            kstack_top: kstack,
            k_cs: kcs.0 as u64,
            k_rflags: k_rflags.bits(),
            k_ss: kss.0 as u64,
        }
    }
}

pub fn allocate_kstack() -> Result<VirtAddr, ThreadingError> {
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
    let start = (base + Size4KiB::SIZE).align_up(Size4KiB::SIZE);
    let end = (base + KSTACK_SIZE as u64).align_up(Size4KiB::SIZE);

    {
        map_region(
            start,
            (end - start) as usize - 1,
            flags,
            &mut *PAGETABLE.lock(),
        )
        .map_err(|_| ThreadingError::StackNotBuilt)?;
    }
    let stack_top = VirtAddr::new((end.as_u64() - 8) & !0xF);
    Ok(stack_top)
}

pub fn free_kstack(top: VirtAddr) -> Result<(), ThreadingError> {
    // assuming top is a properly aligned addr in the correct region
    let start = (top + 1 - KSTACK_SIZE as u64).align_up(Size4KiB::SIZE);
    let idx = (start - KSTACK_AREA_START) as usize / KSTACK_SIZE;

    {
        unmap_region(
            (start + Size4KiB::SIZE).align_up(Size4KiB::SIZE),
            (top - start) as usize,
            &mut *PAGETABLE.lock(),
        )
        .map_err(|_| ThreadingError::StackNotFreed)?;
    }
    *KSTACKS_IN_USAGE
        .lock()
        .get_mut(idx)
        .ok_or(ThreadingError::StackNotFreed)? = false;

    Ok(())
}

pub fn allocate_userstack<M: Mapper<Size4KiB>>(tbl: &mut M) -> Result<VirtAddr, ThreadingError> {
    // all at the same virt addr
    let flags =
        PageTableFlags::WRITABLE | PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    let base = USER_STACK_START.align_up(Size4KiB::SIZE);
    let start = (base + Size4KiB::SIZE).align_up(Size4KiB::SIZE);
    let end = (base + USER_STACK_SIZE as u64).align_up(Size4KiB::SIZE);
    {
        map_region(start, (end - start) as usize - 1, flags, tbl)
            .map_err(|_| ThreadingError::StackNotBuilt)?;
    }

    let stack_top = VirtAddr::new((end.as_u64() - 8) & !0xF);
    Ok(stack_top)
}

pub fn copy_ustack_mappings_into<M: Mapper<Size4KiB>, M2: Mapper<Size4KiB>>(
    from: &mut M,
    into: &mut M2,
) {
    let flags =
        PageTableFlags::WRITABLE | PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    let base = USER_STACK_START.align_up(Size4KiB::SIZE);
    let start = (base + Size4KiB::SIZE).align_up(Size4KiB::SIZE);
    let end = (base + USER_STACK_SIZE as u64).align_up(Size4KiB::SIZE);

    let start_page: Page<Size4KiB> = Page::containing_address(start);
    let end_page: Page<Size4KiB> = Page::containing_address(end);

    {
        let mut frame_allocator = get_frame_alloc().lock();
        for page in Page::range(start_page, end_page) {
            unsafe {
                let frame = from.translate_page(page).unwrap();
                into.map_to(page, frame, flags, &mut *frame_allocator)
                    .unwrap()
                    .flush();
            }
        }
    }
}

pub fn unmap_ustack_mappings(tbl: &mut OffsetPageTable) {
    let base = USER_STACK_START.align_up(Size4KiB::SIZE);
    let start = (base + Size4KiB::SIZE).align_up(Size4KiB::SIZE);
    let end = (base + USER_STACK_SIZE as u64).align_up(Size4KiB::SIZE);

    let start_page: Page<Size4KiB> = Page::containing_address(start);
    let end_page: Page<Size4KiB> = Page::containing_address(end - 1);

    {
        for page in Page::range_inclusive(start_page, end_page) {
            let (_frame, flush) = tbl.unmap(page).unwrap();
            flush.flush();
            // ignore frame, as the frame is still mapped in the users PageTable
        }
    }
}

pub fn free_user_stack(top: VirtAddr, tbl: &mut TaskPageTable) -> Result<(), ThreadingError> {
    // assuming top is at the very top of the user stack
    let start = (top + 1 - USER_STACK_SIZE as u64).align_up(Size4KiB::SIZE);

    unmap_region(
        (start + Size4KiB::SIZE).align_up(Size4KiB::SIZE),
        (top - start) as usize,
        &mut *tbl.table,
    )
    .map_err(|_| ThreadingError::StackNotFreed)
}
