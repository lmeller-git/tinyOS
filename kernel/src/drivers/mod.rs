use tty::start_tty_backend;

pub mod graphics;
pub mod keyboard;
pub mod tty;

pub fn start_drivers() {
    start_tty_backend();
}
