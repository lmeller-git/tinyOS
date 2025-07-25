#![no_std]
#![no_main]
#![allow(
    unused_imports,
    unreachable_code,
    unsafe_op_in_unsafe_fn,
    dead_code,
    unused_doc_comments,
    unused_must_use,
    unused_variables,
    private_interfaces
)]
#![feature(abi_x86_interrupt)]
extern crate alloc;
extern crate tiny_os;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::global_asm;
use embedded_graphics::mono_font;
use embedded_graphics::prelude::Dimensions;
use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics::primitives::StyledDrawable;
use embedded_graphics::text::renderer::TextRenderer;
use os_macros::with_default_args;
use tiny_os::alloc::string::String;
use tiny_os::arch;
use tiny_os::arch::hcf;
use tiny_os::arch::interrupt;
use tiny_os::arch::interrupt::enable_threading_interrupts;
use tiny_os::args;
use tiny_os::bootinfo;
use tiny_os::cross_println;
use tiny_os::drivers::graphics::GLOBAL_FRAMEBUFFER;
use tiny_os::drivers::graphics::colors::ColorCode;
use tiny_os::drivers::graphics::framebuffers::BoundingBox;
use tiny_os::drivers::graphics::framebuffers::FrameBuffer;
use tiny_os::drivers::graphics::framebuffers::LimineFrameBuffer;
use tiny_os::drivers::graphics::text::draw_str;
use tiny_os::drivers::start_drivers;
use tiny_os::exit_qemu;
use tiny_os::get_device;
use tiny_os::include_bins::get_binaries;
use tiny_os::kernel;
use tiny_os::kernel::abi::syscalls::SysRetCode;
use tiny_os::kernel::abi::syscalls::funcs::sys_exit;
use tiny_os::kernel::abi::syscalls::funcs::sys_write;
use tiny_os::kernel::devices::DeviceBuilder;
use tiny_os::kernel::devices::EDebugSinkTag;
use tiny_os::kernel::devices::FdEntry;
use tiny_os::kernel::devices::FdEntryType;
use tiny_os::kernel::devices::GraphicsTag;
use tiny_os::kernel::devices::RawFdEntry;
use tiny_os::kernel::devices::SinkTag;
use tiny_os::kernel::devices::StdInTag;
use tiny_os::kernel::devices::StdOutTag;
use tiny_os::kernel::devices::with_device_init;
use tiny_os::kernel::threading;
use tiny_os::kernel::threading::schedule;
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
use tiny_os::kernel::threading::task::TaskBuilder;
use tiny_os::kernel::threading::task::TaskRepr;
use tiny_os::locks::GKL;
use tiny_os::println;
use tiny_os::serial_print;
use tiny_os::serial_println;
use tiny_os::services::graphics::Glyph;
use tiny_os::services::graphics::PrimitiveGlyph;
use tiny_os::services::graphics::Simplegraphics;
use tiny_os::services::graphics::shapes::Circle;
use tiny_os::services::graphics::shapes::Line;
use tiny_os::services::graphics::shapes::Point;
use tiny_os::services::graphics::shapes::Rect;
use tiny_os::term;
use tiny_os::with_devices;

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    bootinfo::get();
    serial_println!("starting up...");
    kernel::mem::init_paging();
    arch::early_init();
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
    with_devices!(
        |devices| {
            let fb: FdEntry<SinkTag> = DeviceBuilder::tty().fb();
            let serial: FdEntry<EDebugSinkTag> = DeviceBuilder::tty().serial();
            let serial2: FdEntry<StdOutTag> = DeviceBuilder::tty().serial();
            let keyboard: FdEntry<StdInTag> = DeviceBuilder::tty().keyboard();
            // let gfx: FdEntry<GraphicsTag> = DeviceBuilder::gfx().simple();
            let gfx: FdEntry<GraphicsTag> = DeviceBuilder::gfx()
                .blit_kernel(crate::arch::mem::VirtAddr::new(0xffff_ffff_f000_0000));

            devices.attach(fb);
            // devices.attach(serial2);
            devices.attach(serial);
            devices.attach(keyboard);
            devices.attach(gfx);
        },
        || { add_named_ktask(idle, "idle".into()) }
    );
    serial_println!("idle task started");

    enable_threading_interrupts();
    assert!(!GKL.is_locked());
    threading::yield_now();
    unreachable!()
}

#[with_default_args]
extern "C" fn idle() -> usize {
    use core::arch::asm;
    start_drivers();
    threading::finalize();
    serial_println!("threads finalized");

    let mut binaries: Vec<&'static [u8]> = get_binaries();

    serial_println!("adding {} user tasks", binaries.len());
    println!("hi");
    for bin in &binaries {
        let task = TaskBuilder::from_bytes(bin)
            .unwrap()
            .with_default_devices()
            .as_usr()
            .unwrap()
            .build();
        schedule::add_built_task(task);
    }
    serial_println!("{} user tasks added", binaries.len());

    add_named_ktask(grahics, "graphic drawer".into());
    // add_named_ktask(rand, "random".into());
    add_named_ktask(listen, "term".into());
    cross_println!("startup tasks started");

    let x: i64;
    unsafe {
        asm!("push rax", "mov rax, 42", "int 0x80", "mov {0}, rax", "pop rax", out(reg) x, out("rax") _);
    }
    assert_eq!(x, SysRetCode::Unknown as i64);

    // just block forever, as there is nothing left to do
    with_current_task(|task| task.write_inner().block());

    loop {
        threading::yield_now();
    }

    unreachable!()
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
extern "C" fn grahics() -> usize {
    let glyphs = [&PrimitiveGlyph::Circle(
        embedded_graphics::primitives::Circle {
            top_left: embedded_graphics::prelude::Point { x: 250, y: 250 },
            diameter: 42,
        },
        PrimitiveStyle::with_stroke(ColorCode::Pink.into(), 3),
    )];
    get_device!(FdEntryType::Graphics, RawFdEntry::GraphicsBackend(id, backend) => {

        backend.draw_batched_primitives(&glyphs).unwrap();
    });

    let PrimitiveGlyph::Circle(c, s) = glyphs[0] else {
        unreachable!()
    };

    sys_write(
        FdEntryType::Graphics as usize,
        &Into::<BoundingBox>::into(c.bounding_box()) as *const BoundingBox as *const u8,
        1,
    );
    sys_exit(0);

    unreachable!();
    0
}

fn random_stuff() {
    serial_println!("hello from task");

    get_device!(FdEntryType::Graphics, RawFdEntry::GraphicsBackend(id, backend) => {
        backend.draw_glyph(
            &Line {
                start: tiny_os::services::graphics::shapes::Point { x: 0, y: 0 },
                end: tiny_os::services::graphics::shapes::Point { x: 200, y: 200 },
            },
            &tiny_os::drivers::graphics::colors::ColorCode::White,
        );

        backend.draw_glyph(
            &Line {
                start: tiny_os::services::graphics::shapes::Point { x: 400, y: 200 },
                end: tiny_os::services::graphics::shapes::Point { x: 600, y: 0 },
            },
            &tiny_os::drivers::graphics::colors::ColorCode::White,
        );

        backend.draw_glyph(
            &Line {
                start: tiny_os::services::graphics::shapes::Point { x: 400, y: 400 },
                end: tiny_os::services::graphics::shapes::Point { x: 600, y: 600 },
            },
            &tiny_os::drivers::graphics::colors::ColorCode::White,
        );
        backend.draw_glyph(
            &Line {
                start: tiny_os::services::graphics::shapes::Point { x: 200, y: 400 },
                end: tiny_os::services::graphics::shapes::Point { x: 0, y: 600 },
            },
            &tiny_os::drivers::graphics::colors::ColorCode::White,
        );
        backend.draw_glyph(
            &Rect {
                top_left: tiny_os::services::graphics::shapes::Point { x: 200, y: 200 },
                bottom_right: tiny_os::services::graphics::shapes::Point { x: 400, y: 400 },
            },
            &tiny_os::drivers::graphics::colors::ColorCode::Red,
        );
        backend.draw_glyph(
            &Rect {
                top_left: tiny_os::services::graphics::shapes::Point { x: 0, y: 0 },
                bottom_right: tiny_os::services::graphics::shapes::Point { x: 600, y: 600 },
            },
            &tiny_os::drivers::graphics::colors::ColorCode::Green,
        );
        backend.draw_glyph(
            &Rect {
                top_left: tiny_os::services::graphics::shapes::Point { x: 250, y: 250 },
                bottom_right: tiny_os::services::graphics::shapes::Point { x: 350, y: 350 },
            },
            &tiny_os::drivers::graphics::colors::ColorCode::Blue,
        );
        backend.draw_glyph(
            &Circle {
                center: Point { x: 300, y: 300 },
                rad: 50,
            },
            &tiny_os::drivers::graphics::colors::ColorCode::Yellow,
        );

        let s = PrimitiveGlyph::Circle(
            embedded_graphics::primitives::Circle {
                top_left: embedded_graphics::prelude::Point { x: 275, y: 275 },
                diameter: 150,
            },
            PrimitiveStyle::with_stroke(ColorCode::Pink.into(), 2),
        );

        let circle = embedded_graphics::primitives::Circle::new(
            embedded_graphics::prelude::Point::new(275, 275),
            50,
        );
        backend.draw_primitive(&PrimitiveGlyph::Circle(
            circle,
            PrimitiveStyle::with_stroke(ColorCode::Magenta.into(), 1),
        ));

        let builder = embedded_graphics::mono_font::MonoTextStyleBuilder::new()
            .text_color(ColorCode::White.into())
            .background_color(ColorCode::Red.into())
            .font(&mono_font::ascii::FONT_10X20)
            .build();
        backend.draw_primitive(&PrimitiveGlyph::Text(
            &builder,
            "Hello World",
            embedded_graphics::prelude::Point { x: 300, y: 300 },
        ));
        backend.draw_primitive(&PrimitiveGlyph::Text(
            &builder,
            "Hey there",
            embedded_graphics::prelude::Point { x: 300, y: 350 },
        ));
    });

    let bounds = BoundingBox {
        x: 0,
        y: 0,
        width: GLOBAL_FRAMEBUFFER.width(),
        height: GLOBAL_FRAMEBUFFER.height(),
    };

    sys_write(
        FdEntryType::Graphics as usize,
        &bounds as *const BoundingBox as *const u8,
        1,
    );
    cross_println!("copied contents into Global Framebuffer");
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
