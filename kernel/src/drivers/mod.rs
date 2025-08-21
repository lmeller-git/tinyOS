use tty::start_tty_backend;

use crate::drivers::wait_manager::start_wait_managment;

pub mod graphics;
pub mod keyboard;
pub mod tty;
pub mod wait_manager;

pub fn start_drivers() {
    start_tty_backend();
    start_wait_managment();
}
