use tty::start_tty_backend;

use crate::drivers::{resource::start_resource_manager, wait_manager::start_wait_managment};

pub mod graphics;
pub mod keyboard;
pub mod resource;
pub mod tty;
pub mod wait_manager;

pub fn start_drivers() {
    start_tty_backend();
    start_wait_managment();
    start_resource_manager();
}
