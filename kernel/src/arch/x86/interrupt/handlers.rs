use core::{
    arch::global_asm,
    sync::atomic::{AtomicU64, Ordering},
};

pub use x86_64::{
    instructions::port::Port,
    structures::idt::{InterruptStackFrame, PageFaultErrorCode},
};

use crate::{
    arch::{context::SysCallCtx, x86::interrupt::pic::end_interrupt},
    kernel::{abi::syscalls::syscall_handler, threading::schedule::context_switch_local},
    serial_println,
};

static TOTAL_TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

pub(super) extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    // println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
    panic!("breakpoint hit, but not supported: {:?}", stack_frame);
}

pub(super) extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

#[unsafe(no_mangle)]
pub fn timer_interrupt_handler_local_(rsp: u64) {
    // serial_println!("timer");
    assert!(TOTAL_TIMER_TICKS.load(Ordering::Relaxed) < u64::MAX);
    TOTAL_TIMER_TICKS.fetch_add(1, Ordering::Release);
    unsafe { context_switch_local(rsp) }
}

pub fn current_tick() -> u64 {
    TOTAL_TIMER_TICKS.load(Ordering::Acquire)
}

//TODO cleanup
global_asm!(
    "
        .global interrupt_cleanup
        .global timer_interrupt_stub_local
        .global syscall_stub
        .global context_switch_stub

        interrupt_cleanup:
            // reenables interrupts and iretqs
            sti     
            iretq

        timer_interrupt_stub_local:
            // TODO use funcs, maybe only push/pop in switch_and_apply
            cli
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

            // save current rsp
            mov r9, rsp
           
            // align stack, save rsp and save xmm registers
            sub rsp, 512 + 16
            and rsp, -16
            fxsave [rsp]
            push r9
            
            mov rdi, rsp
            call timer_interrupt_handler_local_
            call end_interrupt
            
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
            mov cr3, rax // not necessary, as task not switched
            pop rbx
            pop rcx
            pop rdx
            pop rsi
            pop rdi
            pop rbp
            pop rax
            jmp interrupt_cleanup

        syscall_stub:
            sti
            push rbp
            push r11
            push rcx
            push rbx
            push r8
            push r9
            push r10
            push rdx
            push rsi
            push rdi
            push rax
            
            mov rdi, rsp
            call __syscall_handler

            pop rax
            pop rdi
            pop rsi
            pop rdx
            pop r10
            pop r9
            pop r8
            pop rbx
            pop rcx
            pop r11
            pop rbp
            
            iretq
    "
);

#[unsafe(no_mangle)]
extern "C" fn printer(v: u64) {
    serial_println!("hi from printer, {:#x}", v);
}

unsafe extern "C" {
    pub fn interrupt_cleanup();
    pub fn timer_interrupt_stub_local();
    pub(super) fn syscall_stub();
}

pub(super) extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let mut port = Port::<u8>::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    _ = crate::drivers::keyboard::put_scancode(scancode);
    end_interrupt();
}

pub(super) extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    panic!(
        "EXCEPTION Page fault:\naccessed address: {:?}\nerror code: {:?}\nstack_frame: {:?}",
        Cr2::read(),
        error_code,
        stack_frame
    )
}

pub(super) extern "x86-interrupt" fn gpf_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: GENERAL PROTECTION FAULT\n{:#?}\nError Code: {:b}",
        stack_frame, error_code
    );
}

pub(super) const SPURIOUS_VECTOR: u8 = 0xFF;

pub(super) extern "x86-interrupt" fn spurious_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // nothing to do
    // serial_println!("spurious interrupt");
}

#[unsafe(no_mangle)]
pub(super) extern "C" fn __syscall_handler(ctx: &mut SysCallCtx) {
    syscall_handler(ctx)
}
