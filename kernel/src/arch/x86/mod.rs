// use core::fmt::Write;

use core::{sync::atomic::Ordering, time::Duration};

use x86_64::registers::control::{Cr4, Cr4Flags};

use crate::{
    arch::interrupt::{CYCLES_PER_SECOND, CYCLES_PER_TICK, handlers::current_tick},
    bootinfo::boot_time,
};

pub mod context;
pub mod interrupt;
pub mod mem;
pub mod serial;
pub mod vga;

pub fn early_init() {
    init_xmm();
}

pub fn init() {
    interrupt::init();
    // vga::WRITER.lock().write_str("hello world");
}

fn init_xmm() {
    unsafe {
        Cr4::update(|cr4| {
            cr4.insert(Cr4Flags::OSFXSR);
            cr4.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        });
    }
}

pub fn current_time() -> Duration {
    let total_ticks = current_tick();
    let total_tick_time =
        total_ticks * CYCLES_PER_TICK as u64 / CYCLES_PER_SECOND.load(Ordering::Acquire);
    Duration::from_secs(total_tick_time)
}
