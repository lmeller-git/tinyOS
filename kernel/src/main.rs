#![no_std]
#![no_main]

extern crate tiny_os;

use core::arch::global_asm;
use embedded_graphics::mono_font;
use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics::primitives::StyledDrawable;
use embedded_graphics::text::renderer::TextRenderer;
use os_macros::with_default_args;
use tiny_os::arch;
use tiny_os::arch::hcf;
use tiny_os::arch::interrupt;
use tiny_os::arch::interrupt::enable_threading_interrupts;
use tiny_os::args;
use tiny_os::bootinfo;
use tiny_os::cross_println;
use tiny_os::drivers::graphics::colors::ColorCode;
use tiny_os::drivers::graphics::framebuffers::LimineFrameBuffer;
use tiny_os::drivers::graphics::text::draw_str;
use tiny_os::drivers::start_drivers;
use tiny_os::exit_qemu;
use tiny_os::kernel;
use tiny_os::kernel::threading;
use tiny_os::kernel::threading::schedule::GLOBAL_SCHEDULER;
use tiny_os::kernel::threading::schedule::OneOneScheduler;
use tiny_os::kernel::threading::schedule::add_ktask;
use tiny_os::kernel::threading::schedule::add_named_ktask;
use tiny_os::kernel::threading::schedule::add_named_usr_task;
use tiny_os::kernel::threading::schedule::with_current_task;
use tiny_os::kernel::threading::spawn;
use tiny_os::kernel::threading::spawn_fn;
use tiny_os::kernel::threading::task::Arg;
use tiny_os::kernel::threading::task::Args;
use tiny_os::kernel::threading::task::TaskRepr;
use tiny_os::println;
use tiny_os::serial_print;
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
    serial_println!("starting up...");
    kernel::mem::init_paging();
    serial_println!("paging set up");
    term::init_term();
    cross_println!("terminal started");
    kernel::init_mem();
    cross_println!("heap set up");
    arch::init();
    cross_println!("interrupts set up");
    kernel::init_kernel();
    cross_println!("scheduler initialized");
    cross_println!("OS booted succesfullly");

    #[cfg(feature = "test_run")]
    tiny_os::test_main();
    add_named_ktask(idle, "idle".into());
    serial_println!("idle task started");

    enable_threading_interrupts();
    serial_println!("int: {}", interrupt::are_enabled());
    threading::yield_now();
    unreachable!()
}

#[with_default_args]
extern "C" fn idle() -> usize {
    start_drivers();
    threading::finalize();
    cross_println!("threads finalized");

    add_named_ktask(rand, "random".into());
    add_named_ktask(listen, "term".into());
    cross_println!("startup tasks started");

    // just block forever, as there is nothing left to do
    with_current_task(|task| task.write_inner().block());

    loop {
        threading::yield_now();
    }
    unreachable!()
}

global_asm!(
    "
        .global foo

        foo:
            // mov rdi, rsp
            // call printer2 //0xfffff000c0003ff0
            // sub rsp, 16
            mov rax, 42
            // pop rdi
            // mov rdi, rsp
            // call printer2  //0xfffff000c0003ff8
            // call hcf2
            // jmp rdi
            ret
    ",
);

#[unsafe(no_mangle)]
extern "C" fn hcf2() {
    hcf()
}

#[unsafe(no_mangle)]
extern "C" fn printer2(v: usize) {
    serial_println!("v: {:#x}", v);
}

unsafe extern "C" {
    pub safe fn foo() -> usize;
}

#[with_default_args]
extern "C" fn user_task() -> usize {
    serial_println!("hello from user task");
    hcf();
    0
}

#[with_default_args]
extern "C" fn rand() -> usize {
    serial_println!("hello 0 from task");
    random_stuff();
    0
}

#[with_default_args]
extern "C" fn listen() -> usize {
    tiny_os::term::synced_keyboard_listener();
    0
}

#[with_default_args]
extern "C" fn task1() -> usize {
    // println!("a1: {:#?}", _arg0);
    // let val = unsafe { _arg0.as_val::<&str>() };
    // println!("v: {}", val);
    println!("hello from task 1");
    // panic!("end task1");
    0
}

#[with_default_args]
extern "C" fn task2() -> usize {
    serial_println!("hello from task 2");
    let x: usize;
    unsafe { core::arch::asm!("mov {0}, rsp", out(reg) x) };
    serial_println!("now at: {:#x}", x);
    0
}

fn random_stuff() -> ! {
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
    println!("finished");
    println!("finished2");
    println!("finished3");
    panic!("task random end");
    hcf();
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    #[cfg(feature = "test_run")]
    tiny_os::test_panic_handler(info);
    serial_println!("{:#?}, {}", info, interrupt::are_enabled());
    if GLOBAL_SCHEDULER.is_initialized() {
        if let Some(ref mut sched) = GLOBAL_SCHEDULER
            .get()
            .map(|sched| sched.try_lock().ok())
            .flatten()
        {
            if let Some(current) = sched.current_mut() {
                //TODO kill with info
                current.write_inner().kill_with_code(1);
            }
        }
    }

    arch::hcf()
}
