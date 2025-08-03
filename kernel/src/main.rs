#![no_std]
#![no_main]
#![allow(
    unused_imports,
    unreachable_code,
    unsafe_op_in_unsafe_fn,
    unused_doc_comments,
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
use tiny_os::eprintln;
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
use tiny_os::kernel::threading::schedule::Scheduler;
use tiny_os::kernel::threading::schedule::add_built_task;
use tiny_os::kernel::threading::schedule::add_ktask;
use tiny_os::kernel::threading::schedule::add_named_ktask;
use tiny_os::kernel::threading::schedule::add_named_usr_task;
use tiny_os::kernel::threading::schedule::current_task;
use tiny_os::kernel::threading::schedule::get_scheduler;
use tiny_os::kernel::threading::schedule::with_current_task;
use tiny_os::kernel::threading::spawn;
use tiny_os::kernel::threading::spawn_fn;
use tiny_os::kernel::threading::task::Arg;
use tiny_os::kernel::threading::task::Args;
use tiny_os::kernel::threading::task::TaskBuilder;
use tiny_os::kernel::threading::task::TaskID;
use tiny_os::kernel::threading::task::TaskRepr;
use tiny_os::kernel::threading::tls;
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

    _ = with_devices!(
        |devices| {
            let fb: FdEntry<SinkTag> = DeviceBuilder::tty().fb();
            let serial: FdEntry<EDebugSinkTag> = DeviceBuilder::tty().serial();
            let serial2: FdEntry<StdOutTag> = DeviceBuilder::tty().serial();
            let keyboard: FdEntry<StdInTag> = DeviceBuilder::tty().keyboard();
            let gfx: FdEntry<GraphicsTag> = DeviceBuilder::gfx()
                .blit_kernel(crate::arch::mem::VirtAddr::new(0xffff_ffff_f000_0000));

            devices.attach(fb);
            devices.attach(serial);
            devices.attach(keyboard);
            devices.attach(gfx);
        },
        || { add_named_ktask(idle, "idle".into()) }
    );
    serial_println!("idle task started");
    enable_threading_interrupts();
    threading::yield_now();
    unreachable!()
}

#[with_default_args]
extern "C" fn idle() -> usize {
    use core::arch::asm;

    start_drivers();
    threading::finalize();
    serial_println!("threads finalized");

    _ = add_named_ktask(graphics, "graphic drawer".into());
    _ = add_named_ktask(listen, "term".into());
    cross_println!("startup tasks started");

    let binaries: Vec<&'static [u8]> = get_binaries();

    serial_println!("adding {} user tasks", binaries.len());
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

    get_scheduler().reschedule();

    loop {
        for _ in 0..5 {
            threading::yield_now();
        }
        let scheduler = get_scheduler();
        tls::task_data().cleanup();
        scheduler.reschedule();
        threading::yield_now();
    }
}

#[with_default_args]
extern "C" fn listen() -> usize {
    tiny_os::term::synced_keyboard_listener();
    0
}

#[with_default_args]
extern "C" fn graphics() -> usize {
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

    serial_println!("exiting task {:?}", tls::task_data().current_pid());

    sys_exit(0);

    unreachable!();
    0
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    #[cfg(feature = "test_run")]
    tiny_os::test_panic_handler(info);
    eprintln!(
        "unrecoverable error in task {:?}",
        tls::task_data().current_pid()
    );
    eprintln!("{:#?}, {}", info, interrupt::are_enabled());
    #[cfg(feature = "gkl")]
    {
        if GKL.is_locked() {
            eprintln!("GKL is locked, but the thread is killed.\nUnlocking GKL...");
            unsafe { GKL.unlock_unchecked() };
        }
    }
    if let Ok(current) = current_task() {
        tls::task_data().kill(&tls::task_data().current_pid(), 1);
    }

    loop {
        threading::yield_now();
    }
}
