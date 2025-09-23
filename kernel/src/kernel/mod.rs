pub mod abi;
pub mod devices;
pub mod elf;
pub mod fd;
pub mod fs;
pub mod io;
pub mod mem;
pub mod threading;

pub fn init_mem() {
    mem::init();
}

pub fn init_kernel() {
    threading::init();
    devices::init();
    fs::init();
}
