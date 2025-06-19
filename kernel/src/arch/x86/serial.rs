use core::fmt::{Arguments, Write};

use crate::{arch::interrupt, locks::primitive::Mutex};
use lazy_static::lazy_static;
use uart_16550::SerialPort;

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
        .map_err(|_| SerialErr::IsLocked)?
}

#[derive(Debug, Clone)]
pub enum SerialErr {
    IsLocked,
    WriteErr,
}

#[doc(hidden)]
pub fn _raw_print(args: Arguments) {
    //TODO
    // assert!(SERIAL1.try_lock().is_ok());
    _ = _try_print(args);
    return;
    use core::fmt::Write;
    assert!(!interrupt::are_enabled());
    if let Ok(mut s) = SERIAL1.try_lock() {
        s.write_fmt(args).expect("Printing to serial failed")
    } else {
        unsafe { SERIAL1.force_unlock() }
        _print(args);
        unsafe { SERIAL1.force_lock() }
    }
}
