pub mod devices;
pub mod mem;
pub mod threading;

pub fn init_mem() {
    mem::init();
}

pub fn init_kernel() {
    threading::init();
    devices::init();
}
