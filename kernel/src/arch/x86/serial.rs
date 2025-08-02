use core::fmt::{Arguments, Write};

use crate::{arch::interrupt, sync::locks::Mutex};
use lazy_static::lazy_static;
use uart_16550::SerialPort;
use x86_64::instructions::interrupts::without_interrupts;

lazy_static! {
    static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: Arguments) {
    use core::fmt::Write;
    SERIAL1
        .lock()
        .write_fmt(args)
        .expect("Printing to serial failed")
}

#[doc(hidden)]
pub fn _try_print(args: Arguments) -> Result<(), SerialErr> {
    SERIAL1
        .try_lock()
        .map(|mut s| s.write_fmt(args).map_err(|_| SerialErr::WriteErr))
        .ok_or(SerialErr::IsLocked)?
}

#[derive(Debug, Clone)]
pub enum SerialErr {
    IsLocked,
    WriteErr,
}

#[doc(hidden)]
pub fn _raw_print(slice: &[u8]) {
    let mut lock = SERIAL1.lock();
    for byte in slice {
        lock.send(*byte);
    }
}

// SAFETY: This function is safe, if only this thread accesses SERIAL1
#[doc(hidden)]
pub unsafe fn _force_raw_print(slice: &[u8]) {
    without_interrupts(|| {
        let lock = unsafe { &mut *SERIAL1.data_ptr() };
        for byte in slice {
            lock.send(*byte);
        }
    })
}

// SAFETY: This function is safe, if only this thread accesses SERIAL1
#[doc(hidden)]
pub unsafe fn _force_print(input: Arguments) {
    without_interrupts(|| {
        let guard = unsafe { &mut *SERIAL1.data_ptr() };
        _ = guard.write_fmt(input);
    })
}
