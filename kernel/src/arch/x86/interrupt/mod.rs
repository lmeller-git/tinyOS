pub use x86_64::instructions::interrupts::{are_enabled, without_interrupts};

use crate::println;
pub mod gdt;
pub mod handlers;
mod idt;
mod pic;
use core::arch::asm;

pub use pic::*;

pub(super) fn init() {
    gdt::init();
    println!("gdt");
    idt::init();
    println!("idt");
    pic::init_apic();
    println!("pic");
    // unsafe { handlers::PICS.lock().initialize() };
    x86_64::instructions::interrupts::enable();
    println!("done");
}

pub fn enable_threading_interrupts() {
    enable_timer();
}

pub unsafe fn enable() {
    unsafe { asm!("sti") }
}

pub unsafe fn disable() {
    unsafe { asm!("cli") }
}

pub fn timer() {
    unsafe { core::arch::asm!("int 0x20") }
}
