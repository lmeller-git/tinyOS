use limine::memory_map::EntryType;

use crate::requests::*;

pub fn get() {
    assert!(BASE_REVISION.is_supported());
}

pub fn stack_size() -> u64 {
    if STACK_SIZE_REQUEST.get_response().is_some() {
        STACK_SIZE_REQUEST.size()
    } else {
        // TODO default??
        1024
    }
}

pub struct UsableMRegion {
    pub start: u64,
    pub length: u64,
}

pub fn mmap_entries() {
    // MMAP_REQUEST
}

pub fn usbale_mmap_entries() -> impl Iterator<Item = UsableMRegion> {
    MMAP_REQUEST
        .get_response()
        .expect("failed to get mmap")
        .entries()
        .iter()
        .filter_map(|e| match e.entry_type {
            EntryType::USABLE => Some(UsableMRegion {
                start: e.base,
                length: e.length,
            }),
            _ => None,
        })
}
