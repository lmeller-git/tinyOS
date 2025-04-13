// use core::fmt::Write;

mod interrupt;
//TODO init paging first
// mod vga;

pub fn init() {
    interrupt::init();
    // vga::WRITER
    // .lock()
    // .write_str("Initializing...\n")
    // .expect("could not initialize vga buffer");
}
