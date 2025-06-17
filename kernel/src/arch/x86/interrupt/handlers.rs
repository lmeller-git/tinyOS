use crate::{
    arch::x86::interrupt::pic::end_interrupt,
    kernel::threading::schedule::{context_switch, context_switch_local},
    serial_println,
};
use core::arch::global_asm;
use x86_64::instructions::interrupts::without_interrupts;
pub use x86_64::{
    instructions::port::Port,
    structures::idt::{InterruptStackFrame, PageFaultErrorCode},
};

pub(super) extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    // println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
    panic!("breakpoint hit, but not supported: {:?}", stack_frame);
}

pub(super) extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

#[unsafe(no_mangle)]
pub fn timer_interrupt_handler_local_(rsp: u64) {
    // serial_println!("timer");
    unsafe { context_switch_local(rsp) }
}

//TODO cleanup
global_asm!(
    "
        .global interrupt_cleanup
        .global timer_interrupt_stub_local

        interrupt_cleanup:
            // reenables interrupts, signals eoi and iretqs
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
            mov rdi, rsp
            call timer_interrupt_handler_local_
            call end_interrupt
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
    "
);

#[unsafe(no_mangle)]
extern "C" fn printer(v: u64) {
    serial_println!("hi from printer, {:#x}", v);
}

unsafe extern "C" {
    pub fn interrupt_cleanup();
    pub fn timer_interrupt_stub_local();
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
