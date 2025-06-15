#[cfg(target_arch = "x86_64")]
pub mod x86;
#[cfg(target_arch = "x86_64")]
pub use x86::{context, interrupt, mem};

use core::{arch::asm, fmt::Arguments};

pub fn init() {
    #[cfg(target_arch = "x86_64")]
    x86::init();
    #[cfg(not(any(target_arch = "x86_64")))]
    compile_error!("arch not supported")
}

pub fn hcf() -> ! {
    loop {
        hlt()
    }
}

pub fn timer() {
    #[cfg(target_arch = "x86_64")]
    x86::interrupt::timer();
    #[cfg(not(any(target_arch = "x86_64")))]
    compile_error!("arch not supported")
}

pub fn hlt() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        asm!("hlt");
        #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
        asm!("wfi");
        #[cfg(target_arch = "loongarch64")]
        asm!("idle 0");
    }
}

pub fn current_page_tbl() -> (x86::mem::PhysFrame<x86::mem::Size4KiB>, x86::mem::Cr3Flags) {
    #[cfg(target_arch = "x86_64")]
    return x86::mem::Cr3::read();
    #[cfg(not(any(target_arch = "x86_64")))]
    compile_error!("arch not supported")
}

#[doc(hidden)]
pub fn _serial_print(args: Arguments) {
    #[cfg(target_arch = "x86_64")]
    x86::serial::_print(args);
    #[cfg(not(any(target_arch = "x86_64")))]
    compile_error!("arch not supported")
}
