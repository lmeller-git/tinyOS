use alloc::{format, vec::Vec};
use core::fmt::Arguments;

use super::TTYSink;
use crate::{
    arch::{self},
    drivers::{keyboard::parse_scancode, tty::map_key},
    kernel::{
        fd::{STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO},
        io::Read,
        threading::{self, task::TaskRepr, tls},
    },
    term,
};

//TODO write a macro for these (and others)
pub fn __write_stdout(input: Arguments) {
    if !threading::is_running() {
        term::_print(input);
    } else {
        let bytes = format!("{}", input);
        let bytes = bytes.as_bytes();

        tls::task_data()
            .current_thread()
            .unwrap()
            .fd(STDOUT_FILENO)
            .unwrap()
            .write_continuous(bytes)
            .unwrap();
    }
}

pub fn __write_stderr(input: Arguments) {
    let bytes = format!("{}", input);
    let bytes = bytes.as_bytes();

    tls::task_data()
        .current_thread()
        .unwrap()
        .fd(STDERR_FILENO)
        .unwrap()
        .write_continuous(bytes)
        .unwrap();
}

pub fn __write_debug(input: &str) {
    todo!()
}

// force prints something to serial
pub fn __serial_stub(input: Arguments) {
    arch::_serial_print(input);
    // if interrupt::are_enabled() {
    //     let slice = format!("{}", input);
    //     unsafe { arch::_force_raw_serial_print(slice.as_bytes()) };
    // } else {
    //     arch::_serial_print(input);
    // }
}
