use alloc::{boxed::Box, sync::Arc, vec::Vec};
use conquer_once::spin::OnceCell;
use core::{array, fmt::Debug, marker::PhantomData, sync::atomic::AtomicPtr};
use os_macros::{FDTable, fd_composite_tag};
use tty::{TTYBuilder, TTYSink, TTYSource};

use crate::locks::reentrant::Mutex;

pub mod tty;

// TODO rewrite using cgp from start
//
// currently:
// Visitor:
// FdEntry<T> implements Attacheable or CompositeAttacheable, which gets called by Taskdevices::attach/attach_composite
//
// want:
// FdEntry<T> has method attach, which calls the method of its cgpprovider

static DEFAULT_DEVICES: OnceCell<Mutex<Box<dyn Fn(&mut TaskDevices)>>> = OnceCell::uninit();

#[derive(Debug)]
pub struct TaskDevices {
    // could use HashMap instead for sparse FdEntryTypes
    fd_table: [Option<RawFdEntry>; DEVICE_NUM],
}

impl TaskDevices {
    pub fn get(&self, entry_type: FdEntryType) -> &Option<RawFdEntry> {
        self.fd_table.get(entry_type as usize).unwrap()
    }

    pub fn get_mut(&mut self, entry_type: FdEntryType) -> &mut Option<RawFdEntry> {
        self.fd_table.get_mut(entry_type as usize).unwrap()
    }

    pub fn empty() -> Self {
        Self {
            fd_table: array::from_fn(|_| None),
        }
    }

    pub fn new() -> Self {
        Self::empty()
    }

    pub fn add_default(mut self) -> Self {
        let func = DEFAULT_DEVICES.get().unwrap().lock();
        func(&mut self);
        self
    }

    pub fn attach<T>(&mut self, entry: FdEntry<T>)
    where
        FdEntry<T>: Attacheable,
        T: FdTag,
    {
        entry.attach_to(self);
    }

    pub fn attach_composite<T>(&mut self, entry: FdEntry<T>)
    where
        FdEntry<T>: CompositeAttacheable,
        T: FdTag,
    {
        entry.attach_all(self);
    }

    pub fn replace<T>(&mut self, entry: FdEntry<T>)
    where
        FdEntry<T>: Attacheable,
        T: FdTag,
    {
        todo!()
    }

    pub fn replace_composite<T>(&mut self, entry: FdEntry<T>)
    where
        FdEntry<T>: CompositeAttacheable,
        T: FdTag,
    {
        todo!()
    }
}

pub trait FdTag: Sized + Debug + Clone + Copy + PartialEq + Eq {}

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

#[derive(Clone, Debug)]
pub struct FdEntry<T>
where
    T: FdTag,
{
    inner: RawFdEntry,
    _phantom_type: PhantomData<T>,
}

impl<T: FdTag> FdEntry<T> {
    pub fn new(inner: RawFdEntry) -> Self {
        Self {
            inner,
            _phantom_type: PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
#[repr(usize)]
enum RawFdEntry {
    TTYSink(Vec<Arc<dyn TTYSink>>),
    TTYSource(Vec<Arc<dyn TTYSource>>),
}

impl RawFdEntry {
    pub fn add(&mut self, entry: RawFdEntry) {
        match self {
            Self::TTYSink(own) => {
                let RawFdEntry::TTYSink(s) = entry else {
                    unreachable!()
                };
                own.extend_from_slice(&s);
            }
            Self::TTYSource(own) => {
                let RawFdEntry::TTYSource(s) = entry else {
                    unreachable!()
                };
                own.extend_from_slice(&s);
            }
        }
    }
}

const DEVICE_NUM: usize = 4;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, FDTable)]
pub enum FdEntryType {
    StdIn,
    StdOut,
    StdErr,
    DebugSink,
}

#[fd_composite_tag(DebugSink, StdErr, StdOut)]
pub struct SinkTag;

#[fd_composite_tag(StdErr, StdOut)]
struct SinkTagCopy;

pub struct DeviceBuilder {}

impl DeviceBuilder {
    pub fn tty() -> TTYBuilder {
        TTYBuilder {}
    }
}

pub fn foo() {
    let mut devices = TaskDevices::new();

    let keyboard_entry: FdEntry<StdInTag> = DeviceBuilder::tty().keyboard();
    devices.attach(keyboard_entry);

    let serial: FdEntry<SinkTag> = DeviceBuilder::tty().serial();
    devices.attach(serial);

    let fb: FdEntry<StdInTag> = DeviceBuilder::tty().fb();
    devices.attach(fb);
}

pub fn init() {
    tty::init();
    init_default();
}

fn init_default() {
    DEFAULT_DEVICES.init_once(|| {
        Mutex::new(Box::new(|devices| {
            let sink: FdEntry<SinkTag> = DeviceBuilder::tty().fb();
            let source: FdEntry<StdInTag> = DeviceBuilder::tty().serial();
            devices.attach(sink);
            devices.attach(source);
        }))
    });
}

pub fn get_default_device_init() -> Option<&'static Mutex<Box<dyn Fn(&mut TaskDevices)>>> {
    DEFAULT_DEVICES.get()
}

pub fn with_current_device_list<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&TaskDevices) -> R,
{
    let binding = crate::kernel::threading::schedule::current_task().ok()?;
    let tasks = &binding.read_inner().devices;
    Some(f(tasks))
}
pub fn with_device_init<F, R>(init: Box<dyn Fn(&mut TaskDevices)>, f: F) -> Option<R>
where
    F: FnOnce() -> R,
{
    use core::mem;
    let mut guard = get_default_device_init()?.lock();
    let old = mem::replace(&mut *guard, init);

    let r = f();

    *guard = old;
    Some(r)
}

#[macro_export]
macro_rules! add_device {
    () => {};
}

#[macro_export]
macro_rules! set_device {
    () => {};
}

#[macro_export]
macro_rules! get_device {
    // get device and use it
    ($device_type:expr, $device:pat => $body:block) => {
        crate::kernel::devices::with_current_device_list(|devices| {
            if let Some(devices) = devices.get($device_type) {
                let $device = devices else { unreachable!() };
                $body
            }
        })
    };

    // get device with fallback if no device available
    ($device_type:expr, $device:pat => $body:block | $fallback:block) => {
        crate::kernel::devices::with_current_device_list(|devices| {
            if let Some(devices) = devices.get($device_type) {
                let $device = devices else { unreachable!() };
                $body
            } else {
                $fallback
            }
        })
    };
}

#[macro_export]
macro_rules! with_devices {
    ($func:expr) => {
        crate::kernel::devices::with_device_init(alloc::boxed::Box::new(|_| {}), $func)
    };
    ($init:expr, $func:expr) => {
        crate::kernel::devices::with_device_init(alloc::boxed::Box::new($init), $func)
    };
}

mod tests {
    use os_macros::kernel_test;

    use crate::serial_println;

    use super::*;

    // #[kernel_test]
    fn attach() {
        let mut devices = TaskDevices::empty();
        let sink: FdEntry<SinkTag> = DeviceBuilder::tty().serial();
        devices.attach_composite(sink.clone());

        let sink2: FdEntry<StdOutTag> = DeviceBuilder::tty().fb();
        devices.attach(sink2.clone());

        let RawFdEntry::TTYSink(stdin) = devices.get(FdEntryType::StdIn).as_ref().unwrap() else {
            unreachable!()
        };
        let RawFdEntry::TTYSink(stderr) = devices.get(FdEntryType::StdErr).as_ref().unwrap() else {
            unreachable!()
        };
        let RawFdEntry::TTYSink(debug) = devices.get(FdEntryType::DebugSink).as_ref().unwrap()
        else {
            unreachable!()
        };
        let RawFdEntry::TTYSink(inner) = sink.inner else {
            unreachable!()
        };
        let RawFdEntry::TTYSink(inner2) = sink2.inner else {
            unreachable!()
        };

        serial_println!("{:#?}", devices);

        assert!(Arc::ptr_eq(stdin.get(0).unwrap(), inner.get(0).unwrap()));
        assert!(Arc::ptr_eq(stderr.get(0).unwrap(), inner.get(0).unwrap()));
        assert!(Arc::ptr_eq(debug.get(0).unwrap(), inner.get(0).unwrap()));
        assert!(Arc::ptr_eq(stdin.get(1).unwrap(), inner2.get(0).unwrap()));
    }
}
