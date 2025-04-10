#![no_std]
#![no_main]

mod requests;

use core::arch::asm;
use requests::{BASE_REVISION, FRAMEBUFFER_REQUEST};

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // All limine requests must also be referenced in a called function, otherwise they may be
    // removed by the linker.

    assert!(BASE_REVISION.is_supported());
    #[cfg(feature = "test_run")]
    tiny_os::test_main();

    if let Some(framebuffer_response) = FRAMEBUFFER_REQUEST.get_response() {
        if let Some(framebuffer) = framebuffer_response.framebuffers().next() {
            for i in 0..100_u64 {
                // Calculate the pixel offset using the framebuffer information we obtained above.
                // We skip `i` scanlines (pitch is provided in bytes) and add `i * 4` to skip `i` pixels forward.
                let pixel_offset = i * framebuffer.pitch() + i * 4;

                // Write 0xFFFFFFFF to the provided pixel offset to fill it white.
                unsafe {
                    framebuffer
                        .addr()
                        .add(pixel_offset as usize)
                        .cast::<u32>()
                        .write(0xFFFFFFFF)
                };
            }
        }
    }
    hcf();
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    #[cfg(feature = "test_run")]
    tiny_os::test_panic_handler(_info);
    hcf();
}

fn hcf() -> ! {
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
