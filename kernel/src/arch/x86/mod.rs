// use core::fmt::Write;

pub mod context;
pub mod interrupt;
pub mod mem;
pub mod serial;
pub mod vga;

pub fn init() {
    interrupt::init();
    // vga::WRITER.lock().write_str("hello world");
}
