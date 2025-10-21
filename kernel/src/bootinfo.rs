use core::time::Duration;

use lazy_static::lazy_static;
use limine::{
    framebuffer::Framebuffer,
    memory_map::{Entry, EntryType},
};

use crate::requests::*;

pub fn get() {
    assert!(BASE_REVISION.is_supported());
}

pub fn stack_size() -> u64 {
    if STACK_SIZE_REQUEST.get_response().is_some() {
        STACK_SIZE_REQUEST.size()
    } else {
        4096 * 5
    }
}

pub struct UsableMRegion {
    pub start: u64,
    pub length: u64,
}

pub fn boot_time() -> Duration {
    BOOT_TIME_REQUEST.get_response().unwrap().timestamp()
}

pub fn rdsp_addr() -> usize {
    RSDP_REQUEST.get_response().unwrap().address()
}

pub fn usable_mmap_entries() -> impl Iterator<Item = UsableMRegion> {
    MMAP_REQUEST
        .get_response()
        .expect("could not get response")
        .entries()
        .iter()
        .filter_map(|e| match e.entry_type {
            EntryType::USABLE => Some(UsableMRegion {
                start: if e.base >= 0x100000000 {
                    e.base + get_phys_offset()
                } else {
                    e.base
                },
                length: e.length,
            }),
            _ => None,
        })
}

/// returns the result of hhdm request, ie the virtual address, at which the kernel mapping starts
pub fn get_phys_offset() -> u64 {
    HHDM_REQUEST
        .get_response()
        .expect("could not get physical offset")
        .offset()
}

pub fn get_framebuffers() -> Option<impl Iterator<Item = Framebuffer<'static>>> {
    FRAMEBUFFER_REQUEST.get_response().map(|r| r.framebuffers())
}

lazy_static! {
    pub static ref FIRST_FRAMEBUFFER: Framebuffer<'static> = FRAMEBUFFER_REQUEST
        .get_response()
        .and_then(|f| f.framebuffers().next())
        .unwrap();
}

lazy_static! {
    pub static ref MMAP_ENTRIES: &'static [&'static Entry] =
        MMAP_REQUEST.get_response().unwrap().entries();
}
