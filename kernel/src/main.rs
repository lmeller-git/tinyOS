#![no_std]
#![no_main]
#![allow(
    unreachable_code,
    unsafe_op_in_unsafe_fn,
    unused_doc_comments,
    unused_variables,
    private_interfaces
)]
#![feature(abi_x86_interrupt)]
extern crate alloc;
extern crate tiny_os;

use alloc::{string::String, vec::Vec};
use core::time::Duration;

use embedded_graphics::{prelude::Dimensions, primitives::PrimitiveStyle};
use os_macros::with_default_args;
use tiny_os::{
    arch::{
        self,
        interrupt::{self, enable_threading_interrupts},
        x86::current_time,
    },
    bootinfo,
    cross_println,
    drivers::{
        graphics::{colors::ColorCode, framebuffers::BoundingBox},
        start_drivers,
        wait_manager,
    },
    eprintln,
    get_device,
    include_bins::get_binaries,
    kernel::{
        self,
        abi::syscalls::funcs::{sys_exit, sys_write},
        devices::{
            DeviceBuilder,
            EDebugSinkTag,
            FdEntry,
            FdEntryType,
            GraphicsTag,
            RawFdEntry,
            SinkTag,
            StdInTag,
            StdOutTag,
        },
        fs::{self, OpenOptions, Path, PathBuf},
        io::{Read, Write},
        threading::{
            self,
            schedule::{self, Scheduler, add_named_ktask, current_task, get_scheduler},
            task::{TaskBuilder, TaskRepr},
            tls,
            wait::{QueuTypeCondition, QueueType, condition::WaitCondition},
        },
    },
    locks::GKL,
    serial_println,
    services::graphics::PrimitiveGlyph,
    term,
    with_devices,
};

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
    start_drivers();
    threading::finalize();
    serial_println!("threads finalized");

    _ = add_named_ktask(graphics, "graphic drawer".into());
    // _ = add_named_ktask(listen, "term".into());
    cross_println!("startup tasks started");

    let mut bin_dir: PathBuf = Path::new("/ram/bin").into();
    fs::mkdir(&bin_dir).expect("could not create startup bin dir");

    let binaries: Vec<(String, &'static [u8])> = get_binaries();

    serial_println!("adding {} user tasks", binaries.len());

    for (name, bin) in binaries.iter() {
        bin_dir.push(name.as_str());
        if let Ok(file) = fs::open(&bin_dir, OpenOptions::CREATE) {
            file.write_all(bin, 0)
                .expect("could not write bin to file {name}");
        } else {
            eprintln!("failed to add dir {}", name);
        };
        bin_dir.up();
    }
    let mut bin = Vec::new();
    for (name, _bin) in &binaries {
        bin_dir.push(name.as_str());
        if let Ok(file) = fs::open(&bin_dir, OpenOptions::READ)
            && let Ok(n_read) = file.read_to_end(&mut bin, 0)
        {
            bin_dir.up();
            let task = TaskBuilder::from_bytes(&bin[..n_read])
                .unwrap()
                .with_default_devices()
                .as_usr()
                .unwrap()
                .build();
            serial_println!("task {:?} added", task.pid());
            schedule::add_built_task(task);
        } else {
            eprintln!("could not open or read binary of task {name}");
        }
    }
    serial_println!("{} user tasks added", binaries.len());

    get_scheduler().reschedule();

    loop {
        cross_println!("idle, time: {:?}", current_time());
        let conditions = &[QueuTypeCondition::with_cond(
            QueueType::Timer,
            WaitCondition::Time(Duration::from_secs(5) + current_time()),
        )];
        wait_manager::add_wait(&tls::task_data().current_pid(), conditions);
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
    if !interrupt::are_enabled() {
        serial_println!("panicked wiht disabled interrupts. Trying to recover...");
        unsafe {
            interrupt::enable();
        }
    }

    #[cfg(not(feature = "test_run"))]
    serial_println!("panic: {:#?}", info);

    #[cfg(feature = "test_run")]
    tiny_os::test_panic_handler(info);
    eprintln!(
        "unrecoverable error in task {:?}",
        tls::task_data().current_pid()
    );
    eprintln!(
        "{:#?}, interrupts are currently enabled: {}",
        info,
        interrupt::are_enabled()
    );
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
