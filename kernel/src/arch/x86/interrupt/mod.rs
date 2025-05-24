use crate::println;
pub use x86_64::instructions::interrupts::without_interrupts;
pub mod gdt;
pub mod handlers;
mod idt;
mod pic;
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
