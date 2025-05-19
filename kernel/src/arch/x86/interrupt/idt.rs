use crate::{
    arch::{
        interrupt::handlers::{timer_interrupt_stub, timer_interrupt_stub_local},
        x86::interrupt::handlers::{
            SPURIOUS_VECTOR, breakpoint_handler, double_fault_handler, gpf_handler,
            keyboard_interrupt_handler, page_fault_handler, spurious_interrupt_handler,
            timer_interrupt_handler,
        },
    },
    bootinfo,
};

use super::gdt;
use lazy_static::lazy_static;
use x86_64::{VirtAddr, structures::idt::InterruptDescriptorTable};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);

        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault.set_handler_fn(gpf_handler);
        // unsafe {
        //     idt[InterruptIndex::Timer as u8].set_handler_addr(VirtAddr::new(timer_interrupt_stub as usize as u64));
        // }
        unsafe {
            idt[InterruptIndex::Timer as u8].set_handler_addr(VirtAddr::new(timer_interrupt_stub_local as usize as u64));
        }
        // idt[InterruptIndex::Timer as u8].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard as u8].set_handler_fn(keyboard_interrupt_handler);
        idt[SPURIOUS_VECTOR].set_handler_fn(spurious_interrupt_handler);
        idt
    };
}

pub fn init() {
    IDT.load();
}

#[repr(u8)]
pub enum InterruptIndex {
    Timer = 0x20,
    Keyboard = 0x21,
    // ...
}
