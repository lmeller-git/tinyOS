// use core::fmt::Write;

use x86_64::registers::control::{Cr4, Cr4Flags};

pub mod context;
pub mod cpu;
pub mod interrupt;
pub mod mem;
pub mod serial;
pub mod vga;

pub fn early_init() {
    init_xmm();
}

pub fn init() {
    interrupt::init();
    // vga::WRITER.lock().write_str("hello world");
}

fn init_xmm() {
    unsafe {
        Cr4::update(|cr4| {
            cr4.insert(Cr4Flags::OSFXSR);
            cr4.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        });
    }
}
