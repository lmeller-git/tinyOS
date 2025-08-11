use alloc::format;
use core::fmt::Arguments;

use pc_keyboard::DecodedKey;

use super::TTYSink;
use crate::{
    arch::{self, interrupt},
    drivers::{keyboard::parse_scancode, tty::map_key},
    get_device,
    kernel::{
        devices::{FdEntryType, RawFdEntry},
        threading::{self},
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
}

pub fn read_all(buf: &mut [u8]) -> usize {
    let mut n_read = 0;
    get_device!(FdEntryType::StdIn, RawFdEntry::TTYSource(id, source) => {
     while let Some(next) = source.read()
         && let Ok(res) = parse_scancode(next) {
             let mapped_bytes = map_key(res, buf);
             if mapped_bytes < 0 {
                 break;
             }
             let buf = &mut buf[mapped_bytes as usize..];
             n_read += mapped_bytes as usize;

     }
    });
    n_read
}
