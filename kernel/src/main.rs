#![no_std]
#![no_main]

extern crate tiny_os;

use core::fmt::Write;

use tiny_os::arch;
use tiny_os::bootinfo;
use tiny_os::drivers::graphics::framebuffers::LimineFrameBuffer;
use tiny_os::kernel;
use tiny_os::serial_println;
use tiny_os::services::graphics::Glyph;
use tiny_os::services::graphics::Simplegraphics;
use tiny_os::services::graphics::shapes::Circle;
use tiny_os::services::graphics::shapes::Line;
use tiny_os::services::graphics::shapes::Point;
use tiny_os::services::graphics::shapes::Rect;

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
    let mut fbs = bootinfo::get_framebuffers().unwrap();
    let fb = LimineFrameBuffer::try_new(&mut fbs);
    if let Some(fb) = fb {
        let gfx = Simplegraphics::new(&fb);
        Line {
            start: tiny_os::services::graphics::shapes::Point { x: 0, y: 0 },
            end: tiny_os::services::graphics::shapes::Point { x: 200, y: 200 },
        }
        .render_colorized(&tiny_os::drivers::graphics::colors::ColorCode::White, &gfx);
        Line {
            start: tiny_os::services::graphics::shapes::Point { x: 400, y: 200 },
            end: tiny_os::services::graphics::shapes::Point { x: 600, y: 0 },
        }
        .render_colorized(&tiny_os::drivers::graphics::colors::ColorCode::White, &gfx);
        Line {
            start: tiny_os::services::graphics::shapes::Point { x: 400, y: 400 },
            end: tiny_os::services::graphics::shapes::Point { x: 600, y: 600 },
        }
        .render_colorized(&tiny_os::drivers::graphics::colors::ColorCode::White, &gfx);
        Line {
            start: tiny_os::services::graphics::shapes::Point { x: 200, y: 400 },
            end: tiny_os::services::graphics::shapes::Point { x: 0, y: 600 },
        }
        .render_colorized(&tiny_os::drivers::graphics::colors::ColorCode::White, &gfx);
        Rect {
            top_left: tiny_os::services::graphics::shapes::Point { x: 200, y: 200 },
            bottom_right: tiny_os::services::graphics::shapes::Point { x: 400, y: 400 },
        }
        .render_colorized(&tiny_os::drivers::graphics::colors::ColorCode::Red, &gfx);
        Rect {
            top_left: tiny_os::services::graphics::shapes::Point { x: 0, y: 0 },
            bottom_right: tiny_os::services::graphics::shapes::Point { x: 600, y: 600 },
        }
        .render_colorized(&tiny_os::drivers::graphics::colors::ColorCode::Green, &gfx);
        Rect {
            top_left: tiny_os::services::graphics::shapes::Point { x: 250, y: 250 },
            bottom_right: tiny_os::services::graphics::shapes::Point { x: 350, y: 350 },
        }
        .render_colorized(&tiny_os::drivers::graphics::colors::ColorCode::Blue, &gfx);
        Circle {
            center: Point { x: 300, y: 300 },
            rad: 50,
        }
        .render_colorized(&tiny_os::drivers::graphics::colors::ColorCode::Yellow, &gfx);
    }

    arch::hcf()
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    #[cfg(feature = "test_run")]
    tiny_os::test_panic_handler(info);

    serial_println!("{}", info);
    arch::hcf()
}
