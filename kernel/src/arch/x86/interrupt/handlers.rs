// use super::idt::InterruptIndex;
use crate::{
    arch::{
        context::{ReducedCpuInfo, save_cpu_state},
        x86::interrupt::pic::end_interrupt,
    },
    serial_println,
};
// use pic8259::ChainedPics;
use core::arch::asm;
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

pub(super) extern "x86-interrupt" fn timer_interrupt_handler(mut stack_frame: InterruptStackFrame) {
    // cross_println!("timer");
    // cross_println!("{:#?}", _stack_frame);
    crate::kernel::threading::schedule::switch(&mut stack_frame);
    end_interrupt();
}

pub(super) extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // cross_println!("keyboard");
    let mut port = Port::<u8>::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    _ = crate::drivers::keyboard::put_scancode(scancode);
    let state: *const ReducedCpuInfo;
    // unsafe { asm!("call save_cpu_state ", out("rax") state) };
    // state = unsafe { save_cpu_state() };
    // let state: *const ReducedCpuInfo;
    // unsafe {
    //     asm!(
    //         "push r15",
    //         "push r14",
    //         "push r13",
    //         "push r12",
    //         "push r11",
    //         "push r10",
    //         "push r9",
    //         "push r8",
    //         "push rdi",
    //         "push rsi",
    //         "push rdx",
    //         "push rcx",
    //         "push rbx",
    //         "push rax",
    //         "mov rax, cr3",
    //         "push rax",
    //         "mov {0}, rsp",
    //         out(reg) state,
    //         options(nostack, preserves_flags)
    //     )
    // };
    // serial_println!("calling switch");
    // serial_println!("state_prt: {:#?}, state: {:#?}", state, unsafe {
    //     &(*state)
    // });
    // serial_println!("true frame: {:#?}", _stack_frame);
    // // unsafe { serial_println!("state: {:#?}", *state) };
    // let ptr: *const InterruptStackFrame = &_stack_frame as *const InterruptStackFrame;
    // serial_println!("ptr: {:#?}", ptr);
    // without_interrupts(|| unsafe {
    //     asm!(
    //         "push {0}",
    //         // "mov rdi, {0}",
    //         // "call context_switch_stub",
    //         in(reg) ptr)
    // });

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
        "EXCEPTION: GENERAL PROTECTION FAULT\n{:#?}\nError Code: {}",
        stack_frame, error_code
    );
}

pub(super) const SPURIOUS_VECTOR: u8 = 0xFF;

pub(super) extern "x86-interrupt" fn spurious_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // nothing to do
    // cross_println!("spurious interrupt");
}

// pub(super) const PIC_1_OFFSET: u8 = 32;
// pub(super) const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

// pub(super) static PICS: spin::Mutex<ChainedPics> =
//     spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });
