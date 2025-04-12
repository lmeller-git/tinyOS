mod gdt;
mod handlers;
mod idt;
mod pic;

pub(super) fn init() {
    gdt::init();
    idt::init();
    unsafe { handlers::PICS.lock().initialize() };
    x86_64::instructions::interrupts::enable();
}
