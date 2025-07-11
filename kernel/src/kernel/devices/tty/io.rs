use core::fmt::Arguments;

use super::{
    TTYSink,
    sink::{FBBACKEND, FbBackend, SERIALBACKEND, SerialBackend},
};
use crate::{
    arch::{self, interrupt},
    get_device,
    kernel::{
        devices::{
            FdEntryType, RawFdEntry, tty::source::KEYBOARDBACKEND, with_current_device_list,
        },
        threading::{self, schedule::current_task},
    },
    serial_print, serial_println, term,
};
use alloc::format;
use x86_64::instructions::interrupts::without_interrupts;

//TODO write a macro for these (and others)
pub fn __write_stdout(input: Arguments) {
    if !threading::is_running() {
        term::_print(input);
    } else {
        let bytes = format!("{}", input);
        let bytes = bytes.as_bytes();

        get_device!(FdEntryType::StdOut, RawFdEntry::TTYSink(sinks) => {
            for (k, s) in sinks {
                s.write(bytes)
            }
        });
    }
}

pub fn __write_stderr(input: Arguments) {
    let bytes = format!("{}", input);
    let bytes = bytes.as_bytes();
    get_device!(FdEntryType::StdErr, RawFdEntry::TTYSink(sinks) => {
        for (k, s) in sinks {
            s.write(bytes);
        }
    });
}

pub fn __write_debug(input: &str) {
    let bytes = input.as_bytes();
    get_device!(FdEntryType::DebugSink, RawFdEntry::TTYSink(sinks) => {
        for (k, s) in sinks {
            s.write(bytes);
        }
    });
}

// force prints something to serial
pub fn __serial_stub(input: Arguments) {
    if interrupt::are_enabled() {
        let slice = format!("{}", input);
        unsafe { arch::_force_raw_serial_print(slice.as_bytes()) };
    } else {
        arch::_serial_print(input);
    }
    // if threading::is_running() && interrupt::are_enabled() {
    //     let backend = SERIALBACKEND.get_or_init(SerialBackend::new);
    //     backend.write(format!("{}", input).as_bytes());
    //     backend.flush();
    // } else if !interrupt::are_enabled() {
    //     // the following two MUST NOT ALLOCATE/LOCK (currently they do lock)
    //     arch::_serial_print(input);
    // } else {
    //     arch::_serial_print(input);
    // }
}

pub fn read_all(buf: &mut [u8]) -> usize {
    let mut n_read = 0;
    get_device!(FdEntryType::StdIn, RawFdEntry::TTYSource(id, source) => {
    for i in 0.. buf.len() {
        if let Some(val) = source.read() {
            buf[i] = val;
            n_read += 1;
        } else {
            break;
        }
    }
       });
    n_read
}
