use alloc::{sync::Arc, vec::Vec};
use core::{array, marker::PhantomData};
use os_macros::{FDTable, fd_composite_tag};
use tty::{TTYSink, TTYSource};

pub mod tty;

#[derive(Debug)]
pub struct TaskDevices {
    // could use HashMap instead for sparse FdEntryTypes
    fd_table: [Option<RawFdEntry>; DEVICE_NUM],
}

impl TaskDevices {
    pub fn get(&self, entry_type: FdEntryType) -> &Option<RawFdEntry> {
        todo!()
    }

    pub fn get_mut(&mut self, entry_type: FdEntryType) -> &mut Option<RawFdEntry> {
        todo!()
    }

    pub fn new() -> Self {
        Self {
            fd_table: array::from_fn(|_| None),
        }
    }
}

pub trait Attacheable {
    fn attach_to(self, devices: &mut TaskDevices)
    where
        Self: Sized;
}

pub trait CompositeAttacheable {
    fn attach_all(self, devices: &mut TaskDevices)
    where
        Self: Sized;
}

#[derive(Clone)]
pub struct FdEntry<T> {
    inner: RawFdEntry,
    _phantom_type: PhantomData<T>,
}

#[derive(Debug, Clone)]
enum RawFdEntry {
    TTYSink(Vec<Arc<dyn TTYSink>>),
    TTYSource(Vec<Arc<dyn TTYSource>>),
}

const DEVICE_NUM: usize = 4;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, FDTable)]
pub enum FdEntryType {
    StdIn,
    StdOut,
    StdErr,
    Debug,
}

#[fd_composite_tag(Debug, StdErr, StdOut)]
struct SinkTag;

fn foo() {
    let mut entries = TaskDevices::new();

    let entry = FdEntry {
        inner: RawFdEntry::TTYSource(Vec::new()),
        _phantom_type: PhantomData::<StdInTag>,
    };

    let entry2 = FdEntry {
        inner: RawFdEntry::TTYSink(Vec::new()),
        _phantom_type: PhantomData::<StdOutTag>,
    };

    let entry_clone = FdEntry {
        inner: RawFdEntry::TTYSource(Vec::new()),
        _phantom_type: PhantomData::<StdInTag>,
    };

    entry.attach_to(&mut entries);
    entry2.attach_to(&mut entries);
    let entry = entries.get(FdEntryType::StdIn);
    let entry3 = FdEntry {
        inner: RawFdEntry::TTYSink(Vec::new()),
        _phantom_type: PhantomData::<SinkTag>,
    };

    entry3.attach_all(&mut entries);

    // assert_eq!(entry, &entry_clone);
}

#[macro_export]
macro_rules! get_device {
    () => {
        todo!()
    };
}
