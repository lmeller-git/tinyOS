use conquer_once::spin::OnceCell;
use hashbrown::HashMap;
use thiserror::Error;

use crate::{
    kernel::{
        fd::{FStat, FileRepr, IOCapable},
        fs::{
            FS,
            FSError,
            FSErrorKind,
            FSResult,
            Path,
            PathBuf,
            UnlinkOptions,
            procfs::MaybeRefCounted,
            vfs::VFS,
        },
        io::{Read, Write},
    },
    sync::locks::RwLock,
};

pub static DEVICE_REGISTRY: OnceCell<DeviceRegistry> = OnceCell::uninit();

pub fn init() {
    DEVICE_REGISTRY.init_once(|| DeviceRegistry::new());
    let r: &'static DeviceRegistry = DEVICE_REGISTRY.get().unwrap();
    DEVICE_REGISTRY
        .get()
        .unwrap()
        .register(r, Path::new("/self/registry").into());
    // register_device_file!(DEVICE_REGISTRY.get().unwrap(), "/self/registry");
}

pub fn registry() -> &'static DeviceRegistry {
    DEVICE_REGISTRY.get_or_init(|| DeviceRegistry::new())
}

#[macro_export]
macro_rules! register_device_file {
    ($device:expr, $path:expr) => {{ $crate::kernel::fs::procfs::registry().register($device, $path.into()) }};
}

#[derive(Error, Debug)]
pub enum DeviceRegistryError {
    #[error("the device is already registered. {}", msg)]
    DeviceExists {
        device: DeviceEntry,
        msg: &'static str,
    },
}

#[derive(Debug, Clone)]
pub struct DeviceEntry {
    device: MaybeRefCounted<'static>,
}

impl DeviceEntry {
    pub fn new<T>(device: T) -> Self
    where
        MaybeRefCounted<'static>: From<T>,
    {
        Self {
            device: device.into(),
        }
    }

    pub fn into_inner(self) -> MaybeRefCounted<'static> {
        self.device
    }
}

#[derive(Debug)]
pub struct DeviceRegistry {
    pub(super) devices: RwLock<HashMap<PathBuf, DeviceEntry>>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            devices: RwLock::new(HashMap::new()),
        }
    }

    pub fn register<T>(&self, device: T, path: PathBuf) -> FSResult<()>
    where
        MaybeRefCounted<'static>: From<T>,
    {
        self.devices
            .write()
            .insert(path, DeviceEntry::new(device))
            .map_or(Ok(()), |dev| {
                Err(FSError::custom(
                    FSErrorKind::AlreadyExists,
                    DeviceRegistryError::DeviceExists {
                        device: dev,
                        msg: "the old device was swapped out and returned",
                    }
                    .into(),
                ))
            })
    }

    pub fn deregister(&self, path: &Path) -> FSResult<DeviceEntry> {
        let device = self
            .devices
            .write()
            .remove(path)
            .ok_or(FSError::simple(FSErrorKind::InvalidPath))?;
        if let Some(vfs) = VFS.get() {
            return match vfs.unlink(&path, UnlinkOptions::empty()) {
                Ok(_) => Ok(device),
                Err(err) => {
                    match err.kind() {
                        FSErrorKind::PermissionDenied => {
                            // this operation is not allowed. we should put the device back in place
                            self.devices
                                .write()
                                .insert(path.into(), device)
                                .ok_or(FSError::simple(FSErrorKind::AlreadyExists))?;
                            Err(FSError::simple(FSErrorKind::PermissionDenied))
                        }
                        FSErrorKind::NotFound => {
                            // the device is not yet registered in vfs, nothing to do
                            Ok(device)
                        }
                        e => {
                            // something unexpected happened, for now we just put teh device back and return
                            self.devices
                                .write()
                                .insert(path.into(), device)
                                .ok_or(FSError::simple(FSErrorKind::AlreadyExists))?;
                            Err(FSError::simple(*e))
                        }
                    }
                }
            };
        }
        // vfs does not exist yet
        Ok(device)
    }

    pub fn get(&self, path: &Path) -> FSResult<DeviceEntry> {
        self.devices
            .read()
            .get(path)
            .cloned()
            .ok_or(FSError::simple(FSErrorKind::NotFound))
    }
}

impl FileRepr for DeviceRegistry {
    fn fstat(&self) -> FStat {
        FStat::new()
    }
}

impl IOCapable for DeviceRegistry {}

impl Read for DeviceRegistry {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl Write for DeviceRegistry {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl FileRepr for DeviceEntry {
    fn fstat(&self) -> crate::kernel::fd::FStat {
        FStat::new()
    }
}

impl IOCapable for DeviceEntry {}

impl Read for DeviceEntry {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl Write for DeviceEntry {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}
