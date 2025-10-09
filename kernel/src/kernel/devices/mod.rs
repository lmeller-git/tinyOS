use crate::create_device_file;

pub mod graphics;
pub mod tty;

pub static NULL: Null = Null;
pub const NULL_FILE: &str = "/kernel/null";

fn init_() {
    _ = create_device_file!(&NULL, NULL_FILE);
}

pub fn init() {
    init_();
    tty::init();
    graphics::init();
}

// a placeholder device, which simply does nothing
#[derive(Clone, Copy, Debug, Default)]
pub struct Null;
