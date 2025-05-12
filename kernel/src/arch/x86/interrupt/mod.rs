use crate::println;

pub mod gdt;
mod handlers;
mod idt;
mod pic;

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
