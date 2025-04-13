use pic8259::ChainedPics;
use x86_64::structures::idt::{InterruptStackFrame, PageFaultErrorCode};

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

pub(super) extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer as u8);
    }
}

pub(super) extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // use x86_64::instructions::port::Port;

    // let scancode = unsafe { Port::new(0x60).read() };
    // add_scancode(scancode);

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard as u8);
    }
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

pub(super) const PIC_1_OFFSET: u8 = 32;
pub(super) const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub(super) static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub(super) enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
}
