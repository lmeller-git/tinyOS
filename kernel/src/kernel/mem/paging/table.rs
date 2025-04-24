pub const ENTRY_COUNT: usize = 512;

#[repr(align(4096))]
#[repr(C)]
#[derive(Clone)]
pub struct PageTable {
    entries: [PageTableEntry; ENTRY_COUNT],
}

#[repr(transparent)]
#[derive(Clone)]
pub struct PageTableEntry {
    addr: u64,
}
