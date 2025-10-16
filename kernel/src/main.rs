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

use alloc::sync::Arc;
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
    kernel::{
        self,
        devices::graphics::KERNEL_GFX_MANAGER,
        fd::{File, FileRepr},
        init,
        threading::{
            self,
            schedule::{Scheduler, add_named_ktask, current_task, get_scheduler},
            task::TaskRepr,
            tls,
            wait::{QueuTypeCondition, QueueType, condition::WaitCondition},
        },
    },
    serial_println,
    services::graphics::PrimitiveGlyph,
    term,
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
    kernel::init::early_init();
    cross_println!("heap set up");
    arch::init();
    cross_println!("interrupts set up");
    kernel::init::late_init();
    cross_println!("scheduler initialized");
    cross_println!("OS booted succesfullly");

    #[cfg(feature = "test_run")]
    tiny_os::test_main();

    add_named_ktask(idle, "idle".into()).unwrap();
    serial_println!("idle task started");
    enable_threading_interrupts();
    threading::yield_now();
    unreachable!()
}

#[with_default_args]
extern "C" fn idle() -> usize {
    _ = tls::task_data().get_current().unwrap().add_fd(
        4,
        File::new(KERNEL_GFX_MANAGER.get().unwrap().clone() as Arc<dyn FileRepr>),
    );

    start_drivers();
    threading::finalize();
    serial_println!("threads finalized");

    cross_println!("startup tasks started");

    init::default_task().unwrap();

    serial_println!("default bins started");

    get_scheduler().reschedule();

    serial_println!("entering idle loop...");

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

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    if !interrupt::are_enabled() {
        serial_println!("panicked wiht disabled interrupts. Trying to recover...");
        unsafe {
            interrupt::enable();
        }
    }

    #[cfg(feature = "test_run")]
    tiny_os::test_panic_handler(info);

    serial_println!("panic: {:#?}", info);

    eprintln!(
        "unrecoverable error in task {:?}\nKilling this task...",
        tls::task_data().current_pid()
    );

    if let Ok(current) = current_task() {
        tls::task_data().kill(&tls::task_data().current_pid(), 1);
    }

    loop {
        threading::yield_now();
    }
}
