mod interrupt;
pub mod serial;
pub mod vga;

pub fn init() {
    interrupt::init();
}
