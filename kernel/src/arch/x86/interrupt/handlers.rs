// use super::idt::InterruptIndex;
use crate::{
    arch::{
        context::{
            ReducedCpuInfo, get_context_local, save_context_local, save_reduced_cpu_context,
        },
        hcf,
        x86::interrupt::pic::end_interrupt,
    },
    kernel::threading::schedule::{context_switch, context_switch_local},
    serial_println,
};
// use pic8259::ChainedPics;
use core::arch::{asm, global_asm};
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

#[deprecated]
pub(super) extern "x86-interrupt" fn timer_interrupt_handler(mut stack_frame: InterruptStackFrame) {
    // cross_println!("timer");
    // cross_println!("{:#?}", _stack_frame);
    // crate::kernel::threading::schedule::switch(&mut stack_frame);
    end_interrupt();
}

pub fn timer_interrupt_handler__(frame: InterruptStackFrame, data: ReducedCpuInfo) {
    // serial_println!("hello");
    // serial_println!("{:#?}\n{:#?}", frame, data);
    // hcf();
    without_interrupts(|| unsafe { context_switch(data, frame) });
    end_interrupt();
}

pub fn timer_interrupt_handler_local_(rsp: u64) {
    // serial_println!("timer");
    unsafe { context_switch_local(rsp) }
    // unsafe {
    // context_switch_local();
    // }
    // end_interrupt();
}

//TODO cleanup
global_asm!(
    "
        .global interrupt_cleanup
        .global timer_interrupt_stub
        .global timer_interrupt_stub_local

        interrupt_cleanup:
            // reenables interrupts, signals eoi and iretqs
            // push rdi
            // mov rdi, 1
            // call printer
            // pop rdi
            // mov rdi, rsp
            // call printer
            call {0}
            // push rdi
            // mov rdi, rsp
            // call printer
            // pop rdi
            sti
            iretq

        timer_interrupt_stub_local:
            // TODO use funcs, maybe only push/pop in switch_and_apply
            // call save_context_local
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
            mov rax, rsp
            call {3}
            // push rdi
            // mov rdi, 0
            // call printer
            // pop rdi
            // call get_context_local
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
        
        timer_interrupt_stub:
            /// on entry, the InterruptStackFrame will sit in the stack at rsp
            //TODO call the save func, instead of doing it manually (currently page faults)
            // call {1} // interrupt frame in rax, cpu state in rdx
            // mov rdi, rax
            // mov rsi, rdx
            push rax
            lea rax, [rsp + 8]
            push rbp
            push rdi
            push rsi
            push rdx
            push rcx
            push rbx
            mov rdi, rax
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
            mov rsi, rsp 
            call {2}
            pop r8
            pop r9
            pop r10
            pop r11
            pop r12
            pop r13
            pop r14
            pop r15
            pop rax
            pop rbx
            pop rcx
            pop rdx
            pop rsi
            pop rdi
            pop rbp
            pop rax
            iretq
    ",
    sym end_interrupt,
    sym save_reduced_cpu_context,
    sym timer_interrupt_handler__,
    sym timer_interrupt_handler_local_
);

#[unsafe(no_mangle)]
extern "C" fn printer(v: u64) {
    serial_println!("hi from printer, {:#x}", v);
}

unsafe extern "C" {
    pub(super) fn timer_interrupt_stub();
    pub fn interrupt_cleanup();
    pub(super) fn timer_interrupt_stub_local();
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
        "EXCEPTION: GENERAL PROTECTION FAULT\n{:#?}\nError Code: {:b}",
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
