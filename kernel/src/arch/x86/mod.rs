// use core::fmt::Write;

mod interrupt;
pub mod serial;
pub mod vga;

pub fn init() {
    interrupt::init();
    // vga::WRITER.lock().write_str("hello world");
}
