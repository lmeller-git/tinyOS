use crate::requests::*;
use lazy_static::lazy_static;
use limine::{
    framebuffer::Framebuffer,
    memory_map::{Entry, EntryType},
    response::MemoryMapResponse,
};
use spin::Mutex;

// lazy_static! {
//     static ref MMAP_RESPONSE: Mutex<MemoryMapResponse> =
//         Mutex::new(MMAP_REQUEST.get_response().expect("could nto get mmap"));
// }

pub fn get() {
    assert!(BASE_REVISION.is_supported());
}

// pub const HHDM_OFFSET: u64 = HHDM_REQUEST.get_response().unwrap().offset();

pub fn stack_size() -> u64 {
    if STACK_SIZE_REQUEST.get_response().is_some() {
        STACK_SIZE_REQUEST.size()
    } else {
        // TODO default??
        4096 * 5
    }
}

pub struct UsableMRegion {
    pub start: u64,
    pub length: u64,
}

pub fn rdsp_addr() -> usize {
    RSDP_REQUEST.get_response().unwrap().address()
}

// pub fn mmap_entries<'a>() -> &'a mut [&'a mut Entry] {
//     // MMAP_REQUEST
//     MMAP_RESPONSE.lock().entries_mut()
// }

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
                    // ??
                    e.base //+ get_phys_offset()
                },
                length: e.length,
            }),
            _ => None,
        })
}

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
