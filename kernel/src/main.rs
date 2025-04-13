#![no_std]
#![no_main]

extern crate tiny_os;

use core::fmt::Write;

use tiny_os::arch;
use tiny_os::bootinfo;
use tiny_os::kernel;
use tiny_os::serial_println;

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    bootinfo::get();
    arch::init();
    kernel::init_mem();
    arch::x86::vga::WRITER
        .lock()
        .write_str("Hello world")
        .unwrap();
    #[cfg(feature = "test_run")]
    tiny_os::test_main();

    serial_println!("OS booted succesfully");

    arch::hcf()
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    #[cfg(feature = "test_run")]
    tiny_os::test_panic_handler(info);

    serial_println!("{}", info);
    arch::hcf()
}
