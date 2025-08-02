use alloc::{boxed::Box, sync::Arc, vec::Vec};
use conquer_once::spin::OnceCell;
use core::{
    array,
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, AddAssign},
    sync::atomic::{AtomicPtr, AtomicU64},
};
use graphics::{GFXBuilder, GFXManager};
use hashbrown::HashMap;
use os_macros::{FDTable, fd_composite_tag, kernel_test};
use tty::{TTYBuilder, TTYSink, TTYSource};

use crate::{
    kernel::threading::task::TaskRepr, serial_println, services::graphics::PrimitiveDrawTarget,
    sync::locks::Mutex,
};

pub mod graphics;
pub mod tty;

// TODO rewrite using cgp from start
//
// currently:
// Visitor:
// FdEntry<T> implements Attacheable or CompositeAttacheable, which gets called by Taskdevices::attach/attach_composite
//
// want:
// FdEntry<T> has method attach, which calls the method of its cgpprovider

static DEFAULT_DEVICES: OnceCell<Mutex<Box<dyn Fn(&mut TaskDevices) + Send>>> = OnceCell::uninit();

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

    pub fn attach<T>(&mut self, entry: FdEntry<T>) -> DeviceID<T>
    where
        FdEntry<T>: Attacheable,
        T: FdTag,
    {
        let id = entry.id;
        entry.attach_to(self);
        id
    }

    pub fn attach_composite<T>(&mut self, entry: FdEntry<T>) -> DeviceID<T>
    where
        FdEntry<T>: CompositeAttacheable,
        T: FdTag,
    {
        let id = entry.id;
        entry.attach_all(self);
        id
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

    pub fn is_empty(&self) -> bool {
        !self.fd_table.iter().any(|entry| {
            if let Some(entry) = entry {
                !entry.is_empty()
            } else {
                false
            }
        })
    }
}

pub fn next_device_id() -> RawDeviceID {
    // 0 is reserved for Null
    static CURRENT_DEVICE_ID: AtomicU64 = AtomicU64::new(1);
    let current = CURRENT_DEVICE_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    RawDeviceID::new(current)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct DeviceIDBuilder {
    inner: RawDeviceID,
}

#[allow(dead_code)]
impl DeviceIDBuilder {
    fn new() -> Self {
        Self {
            inner: RawDeviceID::default(),
        }
    }

    fn get_next<T: FdTag>(&mut self) -> DeviceID<T> {
        let new: DeviceID<T> = DeviceID::new(self.inner);
        self.inner.inc();
        new
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct RawDeviceID {
    inner: u64,
}

#[allow(dead_code)]
impl RawDeviceID {
    fn new(num: u64) -> Self {
        Self { inner: num }
    }

    fn inc(&mut self) {
        self.inner += 1;
    }
}

impl AddAssign for RawDeviceID {
    fn add_assign(&mut self, rhs: Self) {
        self.inner += rhs.inner
    }
}

impl Add for RawDeviceID {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            inner: self.inner + rhs.inner,
        }
    }
}

impl From<u64> for RawDeviceID {
    fn from(value: u64) -> Self {
        Self { inner: value }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceID<T: FdTag> {
    inner: RawDeviceID,
    _phantom_tag: PhantomData<T>,
}

impl<T: FdTag> DeviceID<T> {
    fn new(raw_id: RawDeviceID) -> Self {
        Self {
            inner: raw_id,
            _phantom_tag: PhantomData,
        }
    }
}

// impl<T: FdTag> Debug for DeviceID<T> {
//     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//         f.debug_struct("DeviceId")
//             .field("id", &self.inner)
//             .field("type", &core::any::type_name::<T>())
//             .finish()
//     }
// }

pub trait Detacheable {
    fn detach(self, devices: &mut TaskDevices)
    where
        Self: Sized;
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
    id: DeviceID<T>,
    _phantom_type: PhantomData<T>,
}

impl<T: FdTag> FdEntry<T> {
    pub fn new(inner: RawFdEntry, raw_id: RawDeviceID) -> Self {
        Self {
            inner,
            id: DeviceID::new(raw_id),
            _phantom_type: PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
#[repr(usize)]
pub enum RawFdEntry {
    TTYSink(HashMap<RawDeviceID, Arc<dyn TTYSink>>),
    TTYSource(RawDeviceID, Arc<dyn TTYSource>),
    GraphicsBackend(RawDeviceID, Arc<dyn GFXManager>),
}

impl RawFdEntry {
    pub fn add(&mut self, entry: RawFdEntry) {
        match self {
            Self::TTYSink(own) => {
                let RawFdEntry::TTYSink(s) = entry else {
                    unreachable!()
                };
                own.extend(s);
            }
            Self::TTYSource(id1, own) => {
                let RawFdEntry::TTYSource(id2, backend2) = entry else {
                    unreachable!()
                };
                *id1 = id2;
                *own = backend2;
            }
            Self::GraphicsBackend(id1, backend1) => {
                let RawFdEntry::GraphicsBackend(id2, backend2) = entry else {
                    unreachable!()
                };
                *id1 = id2;
                *backend1 = backend2;
            }
        }
    }

    pub fn remove(&mut self, id: RawDeviceID) {
        match self {
            Self::TTYSink(own) => _ = own.remove(&id),
            Self::TTYSource(backend_id, backend) => {
                if *backend_id == id {
                    *backend = Arc::new(Null);
                    *backend_id = RawDeviceID::new(0)
                }
            }
            Self::GraphicsBackend(backend_id, backend) => {
                if *backend_id == id {
                    *backend = Arc::new(Null);
                    *backend_id = RawDeviceID::new(0)
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::TTYSink(_) => self.n_attached() == 0,
            Self::GraphicsBackend(id, _) | Self::TTYSource(id, _) => id.inner == 0, // this indicates that Null is attached
        }
    }

    pub fn n_attached(&self) -> usize {
        match self {
            Self::TTYSink(sinks) => sinks.len(),
            Self::TTYSource(_, _) => 1,
            Self::GraphicsBackend(_, _) => 1,
        }
    }
}

// a placeholder device, which simply does nothing
#[derive(Clone, Copy, Debug, Default)]
pub struct Null;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, FDTable)]
pub enum FdEntryType {
    StdIn,
    StdOut,
    StdErr,
    DebugSink,
    Graphics,
}

#[fd_composite_tag(DebugSink, StdErr, StdOut)]
pub struct SinkTag;

#[fd_composite_tag(StdErr, StdOut)]
struct SinkTagCopy;

#[fd_composite_tag(StdErr, DebugSink)]
pub struct EDebugSinkTag;

#[fd_composite_tag(StdOut, DebugSink)]
pub struct SuccessSinkTag;

#[fd_composite_tag()]
pub struct NullTag;

pub struct DeviceBuilder {}

impl DeviceBuilder {
    pub fn tty() -> TTYBuilder {
        TTYBuilder::new(next_device_id())
    }
    pub fn gfx() -> GFXBuilder {
        GFXBuilder::new(next_device_id())
    }
}

pub fn init() {
    tty::init();
    init_default();
}

fn init_default() {
    DEFAULT_DEVICES.init_once(|| {
        Mutex::new(Box::new(|devices| {
            _ = with_current_device_list(|current_devices| {
                for (i, entry) in current_devices.fd_table.iter().enumerate() {
                    devices.fd_table[i] = entry.clone();
                }
            });
        }))
    });
}

pub fn get_default_device_init() -> Option<&'static Mutex<Box<dyn Fn(&mut TaskDevices) + Send>>> {
    DEFAULT_DEVICES.get()
}

pub fn with_current_device_list<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&TaskDevices) -> R,
{
    let binding = crate::kernel::threading::schedule::current_task().ok()?;
    let tasks = &*binding.devices().read();
    Some(f(tasks))
}

pub fn with_device_init<F, R>(init: Box<dyn Fn(&mut TaskDevices) + Send>, f: F) -> Option<R>
where
    F: FnOnce() -> R,
{
    use core::mem;
    let mut guard = get_default_device_init()?.lock();
    let old = mem::replace(&mut *guard, init);
    drop(guard);

    let r = f();

    let mut guard = get_default_device_init()?.lock();
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
        $crate::kernel::devices::with_current_device_list(|devices| {
            if let Some(devices) = devices.get($device_type) {
                let $device = devices else { unreachable!() };
                $body
            }
        })
    };

    // get device with fallback if no device available
    ($device_type:expr, $device:pat => $body:block | $fallback:block) => {
        $crate::kernel::devices::with_current_device_list(|devices| {
            if let Some(devices) = devices.get($device_type) {
                let $device = devices else { unreachable!() };
                $body
            } else {
                $fallback
            }
        })
    };
    // get devices matching ID
    ($device_id:expr) => {
        todo!()
    };
}

#[macro_export]
macro_rules! with_devices {
    ($func:expr) => {
        $crate::kernel::devices::with_device_init(alloc::boxed::Box::new(|_| {}), $func)
    };
    ($init:expr, $func:expr) => {
        $crate::kernel::devices::with_device_init(alloc::boxed::Box::new($init), $func)
    };
}

mod tests {
    use alloc::{format, string::String};
    use os_macros::kernel_test;

    use crate::{println, serial_println};

    use super::*;

    #[kernel_test(verbose)]
    pub fn basic() {
        let mut devices = TaskDevices::new();
        let keyboard_entry: FdEntry<StdInTag> = DeviceBuilder::tty().keyboard();
        let id = devices.attach(keyboard_entry);

        let gfx: FdEntry<GraphicsTag> = DeviceBuilder::gfx().simple();
        let gfx_id = devices.attach(gfx);

        let serial: FdEntry<SinkTag> = DeviceBuilder::tty().serial();
        let id2 = devices.attach(serial);

        let fb: FdEntry<SinkTag> = DeviceBuilder::tty().fb();
        let id3 = devices.attach(fb);
        let mut s = String::new();
        for _ in 0..200 {
            s.push('_');
        }

        id.detach(&mut devices);
        id2.detach(&mut devices);
        id3.detach(&mut devices);
        gfx_id.detach(&mut devices);
        assert!(devices.is_empty())
    }

    #[kernel_test(verbose)]
    fn attach() {
        let mut devices = TaskDevices::empty();
        let sink: FdEntry<SinkTag> = DeviceBuilder::tty().serial();
        let sink_id = devices.attach_composite(sink.clone());

        let sink2: FdEntry<StdOutTag> = DeviceBuilder::tty().fb();
        let sink2_id = devices.attach(sink2.clone());
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

        // TODO
    }
}
