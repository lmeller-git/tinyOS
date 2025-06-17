use core::fmt::Arguments;

use super::{
    TTYSink,
    sink::{FBBACKEND, FbBackend, SERIALBACKEND, SerialBackend},
};
use crate::{
    arch,
    kernel::{
        devices::{FdEntryType, RawFdEntry, with_current_device_list},
        threading::{self, schedule::current_task},
    },
    serial_print, term,
};
use alloc::format;
use x86_64::instructions::interrupts::without_interrupts;

//TODO write a macro for these (and others)
pub fn __write_stdout(input: Arguments) {
    if !threading::is_running() {
        term::_print(input);
    } else {
        term::_print(input);
        return;
        let bytes = format!("{}", input);
        let bytes = bytes.as_bytes();
        with_current_device_list(|devices| {
            if let Some(devices) = devices.get(FdEntryType::StdOut) {
                let RawFdEntry::TTYSink(sinks) = devices else {
                    unreachable!()
                };
                for s in sinks {
                    s.write(bytes);
                }
            }
        });
    }
}

pub fn __write_stderr(input: &str) {
    let bytes = input.as_bytes();
    with_current_device_list(|devices| {
        if let Some(devices) = devices.get(FdEntryType::StdErr) {
            let RawFdEntry::TTYSink(sinks) = devices else {
                unreachable!()
            };
            for s in sinks {
                s.write(bytes);
            }
        }
    });
}

pub fn __write_debug(input: &str) {
    let bytes = input.as_bytes();
    with_current_device_list(|devices| {
        if let Some(devices) = devices.get(FdEntryType::DebugSink) {
            let RawFdEntry::TTYSink(sinks) = devices else {
                unreachable!()
            };
            for s in sinks {
                s.write(bytes);
            }
        }
    });
}

pub fn __serial_stub(input: Arguments) {
    without_interrupts(|| {
        if threading::is_running() {
            arch::_serial_print(input);
            return;
            let backend = SERIALBACKEND.get_or_init(SerialBackend::new);
            backend.write(format!("{}", input).as_bytes());
            backend.flush();
        } else {
            arch::_serial_print(input);
        }
    })
}
