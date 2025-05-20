#![no_std]
#![no_main]

extern crate tiny_os;

use embedded_graphics::mono_font;
use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics::primitives::StyledDrawable;
use embedded_graphics::text::renderer::TextRenderer;
use tiny_os::arch;
use tiny_os::arch::interrupt::enable_threading_interrupts;
use tiny_os::bootinfo;
use tiny_os::cross_println;
use tiny_os::drivers::graphics::colors::ColorCode;
use tiny_os::drivers::graphics::framebuffers::LimineFrameBuffer;
use tiny_os::drivers::graphics::text::draw_str;
use tiny_os::kernel;
use tiny_os::kernel::threading::schedule::add_ktask;
use tiny_os::println;
use tiny_os::serial_println;
use tiny_os::services::graphics::Glyph;
use tiny_os::services::graphics::Simplegraphics;
use tiny_os::services::graphics::shapes::Circle;
use tiny_os::services::graphics::shapes::Line;
use tiny_os::services::graphics::shapes::Point;
use tiny_os::services::graphics::shapes::Rect;
use tiny_os::term;

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    bootinfo::get();
    // serial_println!("{:x}", bootinfo::get_phys_offset());
    kernel::mem::init_paging();
    term::init_term();
    println!("terminal started");
    kernel::init_mem();
    arch::init();
    kernel::threading::init();
    cross_println!("OS booted succesfullly");

    #[cfg(feature = "test_run")]
    tiny_os::test_main();
    add_ktask(rand).unwrap();
    // add_ktask(listen).unwrap();
    // random_stuff();
    enable_threading_interrupts();
    // serial_println!("unreachable??");
    // serial_println!("wtffftftftftfft");
    // serial_println!("yugfqweifuygweqiuygf");
    arch::hcf()
}

#[unsafe(no_mangle)]
extern "C" fn rand() {
    serial_println!("hello 0 from task");
    random_stuff();
}

#[unsafe(no_mangle)]
extern "C" fn listen() {
    tiny_os::term::synced_keyboard_listener();
}

fn random_stuff() {
    serial_println!("hello from task");
    let mut fbs = bootinfo::get_framebuffers().unwrap();
    let fb = LimineFrameBuffer::try_new(&mut fbs);
    if let Some(fb) = fb {
        let mut gfx = Simplegraphics::new(&fb);
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

        let circle = embedded_graphics::primitives::Circle::new(
            embedded_graphics::prelude::Point::new(275, 275),
            50,
        );
        circle
            .draw_styled(
                &PrimitiveStyle::with_stroke(ColorCode::Magenta.into(), 1),
                &mut gfx,
            )
            .unwrap();
        let builder = embedded_graphics::mono_font::MonoTextStyleBuilder::new()
            .text_color(ColorCode::White.into())
            .background_color(ColorCode::Red.into())
            .font(&mono_font::ascii::FONT_10X20)
            .build();
        builder
            .draw_string(
                "Hello World",
                embedded_graphics::prelude::Point { x: 300, y: 300 },
                embedded_graphics::text::Baseline::Top,
                &mut gfx,
            )
            .unwrap();
        draw_str(
            "helloooeoeoeoe",
            embedded_graphics::prelude::Point { x: 300, y: 350 },
            &mut gfx,
        )
        .unwrap();
    }
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    #[cfg(feature = "test_run")]
    tiny_os::test_panic_handler(info);

    serial_println!("{}", info);
    arch::hcf()
}
