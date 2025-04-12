// use core::fmt::Write;

mod interrupt;
mod paging;
//TODO init paging first
// mod vga;

pub fn init() {
    interrupt::init();
    // vga::WRITER
    // .lock()
    // .write_str("Initializing...\n")
    // .expect("could not initialize vga buffer");
    // SAFETY: this is safe if the provided mmap is safe
    // unsafe { paging::init() };
}
