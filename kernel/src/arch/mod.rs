#[cfg(target_arch = "x86_64")]
mod x86;

use core::arch::asm;

pub fn init() {
    #[cfg(target_arch = "x86_64")]
    x86::init();
    #[cfg(not(target_arch = "x86_64"))]
    panic!("arch not supported")
}

pub fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            asm!("wfi");
            #[cfg(target_arch = "loongarch64")]
            asm!("idle 0");
        }
    }
}
