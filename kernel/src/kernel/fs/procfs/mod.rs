use alloc::{
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
};
use core::{fmt::Display, ops::Deref, ptr::null_mut};

use bitflags::Flags;
use hashbrown::DefaultHashBuilder;
use indexmap::IndexMap;
use thiserror::Error;

use crate::{
    kernel::{
        fd::{FStat, File, FileBuilder, FileRepr, IOCapable},
        fs::{FS, FSError, FSErrorKind, FSResult, OpenOptions, Path, UnlinkOptions},
        io::{Read, Write},
    },
    serial_println,
    sync::locks::RwLock,
};

mod register;
pub use register::*;

const NULL_DEVICE: &'static Null = &Null;

#[derive(Error, Debug)]
pub enum ProcFSError {}

type ProcFilePtr = Arc<ProcFile>;

#[derive(Debug)]
struct ProcFile {
    node: ProcNode,
}

impl ProcFile {
    pub fn new(node: ProcNode) -> Self {
        Self { node }
    }

    fn is_dir(&self) -> bool {
        match self.node {
            ProcNode::Dir(_) => true,
            _ => false,
        }
    }
}

impl FileRepr for ProcFile {
    fn fstat(&self) -> FStat {
        self.node.fstat().unwrap_or_default()
    }

    fn node_type(&self) -> super::NodeType {
        match self.node {
            ProcNode::Dir(_) => super::NodeType::Dir,
            ProcNode::File(_) => super::NodeType::File,
        }
    }

    fn as_raw_parts(&self) -> (*mut u8, usize) {
        match &self.node {
            ProcNode::File(f) => f.as_raw_parts(),
            ProcNode::Dir(d) => (null_mut(), 0),
        }
    }
}

impl IOCapable for ProcFile {}

impl Read for ProcFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        match &self.node {
            ProcNode::Dir(d) => Ok(read_dir(d, buf)), // the Dir d is lazy and will not be updated by this operation. Call flush() prior to ls TODO: automatic update of d (need access to path)
            ProcNode::File(f) => f.read(buf, offset),
        }
    }

    fn read_to_end(
        &self,
        buf: &mut vec::Vec<u8>,
        mut offset: usize,
    ) -> crate::kernel::io::IOResult<usize> {
        match &self.node {
            ProcNode::Dir(d) => {
                let res = format!("{}", d);
                let bytes = res.as_bytes();
                buf.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            ProcNode::File(f) => loop {
                let mut written = 0;
                loop {
                    let count = self.read(&mut buf[written..], offset)?;
                    if count == buf[written..].len() {
                        buf.resize(buf.len().max(1) * 2, 0);
                    } else if count == 0 {
                        return Ok(written);
                    }
                    written += count;
                    offset += count;
                }
            },
        }
    }
}

impl Write for ProcFile {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        match &self.node {
            ProcNode::Dir(_) => Err(FSError::simple(FSErrorKind::NotSupported)),
            ProcNode::File(f) => f.write(buf, offset),
        }
    }
}

fn read_dir(dir: &DirData, buf: &mut [u8]) -> usize {
    dir.bufferd_display(buf, 0).0
}

#[derive(Debug)]
enum ProcNode {
    Dir(DirData),
    File(MaybeRefCounted<'static>), // a File, typically a device, living for 'static or owned by an arc
}

impl ProcNode {
    pub fn new_dir() -> Self {
        Self::Dir(DirData::default())
    }

    pub fn new_file<F>(file: F) -> Self
    where
        MaybeRefCounted<'static>: From<F>,
    {
        Self::File(file.into())
    }

    pub fn fstat(&self) -> Option<FStat> {
        match self {
            Self::Dir(_) => None,
            Self::File(f) => Some(f.fstat()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum MaybeRefCounted<'a> {
    Arc(Arc<dyn FileRepr>),
    Ref(&'a dyn FileRepr),
}

impl<'a> MaybeRefCounted<'a> {
    pub fn new<V>(value: V) -> Self
    where
        MaybeRefCounted<'a>: From<V>,
    {
        value.into()
    }
}

impl<'a, T: FileRepr> From<&'a T> for MaybeRefCounted<'a> {
    fn from(value: &'a T) -> Self {
        (value as &dyn FileRepr).into()
    }
}

// impl<T: FileRepr + 'static> From<T> for MaybeRefCounted<'_> {
//     fn from(value: T) -> Self {
//         Arc::new(value).into()
//     }
// }

impl<'a> From<&'a dyn FileRepr> for MaybeRefCounted<'a> {
    fn from(value: &'a dyn FileRepr) -> Self {
        Self::Ref(value)
    }
}

impl From<Arc<dyn FileRepr>> for MaybeRefCounted<'_> {
    fn from(value: Arc<dyn FileRepr>) -> Self {
        Self::Arc(value)
    }
}

impl<'a, T> From<Arc<T>> for MaybeRefCounted<'a>
where
    T: FileRepr + 'a,
    'a: 'static,
{
    fn from(value: Arc<T>) -> Self {
        (value as Arc<dyn FileRepr + 'a>).into()
    }
}

impl<'a> Deref for MaybeRefCounted<'a> {
    type Target = dyn FileRepr + 'a;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Arc(t) => &**t,
            Self::Ref(t) => *t,
        }
    }
}

type DirInner = IndexMap<String, ProcFilePtr, DefaultHashBuilder>;

#[derive(Debug, Default)]
struct DirData {
    inner: RwLock<DirInner>,
}

impl DirData {
    fn ensure_entry<F>(&self, name: String, f: F) -> ProcFilePtr
    where
        F: FnOnce() -> ProcFilePtr,
    {
        self.inner.write().entry(name).or_insert_with(f).clone()
    }

    fn get_entry(&self, name: &str) -> FSResult<ProcFilePtr> {
        self.inner
            .read()
            .get(name)
            .ok_or(FSError::simple(FSErrorKind::NotFound))
            .cloned()
    }

    fn get_or_update(&self, path: &Path, name: &str) -> FSResult<ProcFilePtr> {
        if let Some(node) = self.inner.read().get(name) {
            // short path: node is already contained in self, nothing to do
            Ok(node.clone())
        } else {
            // node not contained in self, we must check back with the device registry
            let entry = registry().get(path)?;
            let node = proc_file(entry.into_inner());
            self.inner
                .write()
                .insert(name.to_string(), node.clone())
                .map_or(Ok(()), |_| Err(FSError::simple(FSErrorKind::AlreadyExists)))?;
            Ok(node)
        }
    }

    // writes names for all entries in self into buffer, while buffer has space, separated by '\t'. Writes either a whole name + '\t', or nothing
    // returns (_, true) if no entries remain
    fn bufferd_display(&self, buf: &mut [u8], offset: usize) -> (usize, bool) {
        let mut written = 0;
        let mut newly_written = 0;
        for name in self.inner.read().keys() {
            let bytes = name.as_bytes();
            let total_len = bytes.len() + 1;
            if written < offset {
                // skip this entry
                written += total_len;
                continue;
            }
            if total_len + newly_written > buf.len() {
                // no space in buf
                return (newly_written, false);
            }

            // write entry + '\t' into buf
            assert!(buf.len() > newly_written + total_len - 1);
            assert!(bytes.len() == total_len - 1);
            buf[newly_written..newly_written + total_len - 1].copy_from_slice(bytes);
            buf[newly_written + total_len - 1] = b'\t';
            newly_written += total_len;
        }
        (newly_written, true)
    }
}

impl Display for DirData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut buf = vec![0; 10];
        let mut offset = 0;
        loop {
            let (read, is_done) = self.bufferd_display(&mut buf, offset);
            assert!(read <= buf.len());
            match (read, is_done) {
                (0, true) => break,
                (0, false) => buf.resize(buf.len() * 2, 0),
                (n, true) => {
                    let name = str::from_utf8(&buf[..n]).expect("malformed entry in dir");
                    f.write_str(name)?;
                    break;
                }
                (n, false) => {
                    let name = str::from_utf8(&buf[..n]).expect("malformed entry in dir");
                    f.write_str(name)?;
                    offset += n;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Null;

impl FileRepr for Null {
    fn fstat(&self) -> FStat {
        FStat::new()
    }

    fn node_type(&self) -> super::NodeType {
        super::NodeType::Void
    }
}

impl IOCapable for Null {}

impl Read for Null {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        Err(FSError::simple(FSErrorKind::NotSupported))
    }
}

impl Write for Null {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        Err(FSError::simple(FSErrorKind::NotSupported))
    }
}

pub fn proc_file<F>(device_file: F) -> ProcFilePtr
where
    MaybeRefCounted<'static>: From<F>,
{
    ProcFilePtr::new(ProcFile::new(ProcNode::new_file(device_file)))
}

pub fn empty_proc_file() -> ProcFilePtr {
    proc_file(NULL_DEVICE)
}

pub fn proc_dir() -> ProcFilePtr {
    ProcFilePtr::new(ProcFile::new(ProcNode::new_dir()))
}

fn as_file(ptr: ProcFilePtr) -> FileBuilder {
    FileBuilder::new(ptr as Arc<dyn FileRepr>)
}

fn with_dir<F, R>(parent: ProcFilePtr, func: F) -> FSResult<R>
where
    F: FnOnce(&DirData) -> R,
{
    let ProcNode::Dir(ref d) = parent.node else {
        return Err(FSError::simple(FSErrorKind::InvalidPath));
    };
    Ok(func(d))
}

#[derive(Debug)]
pub struct ProcFS {
    root: ProcFilePtr,
}

impl ProcFS {
    pub fn new() -> Self {
        Self { root: proc_dir() }
    }

    fn traverse(&self, path: &Path, create: bool) -> FSResult<ProcFilePtr> {
        let mut current_dir = self.root.clone();
        // skip last (target) component
        let Some(parent) = path.parent() else {
            return Ok(current_dir);
        };

        // skip root dir
        for component in parent.traverse().skip(1) {
            let child = if create {
                with_dir(current_dir.clone(), |dir| {
                    dir.ensure_entry(component.to_string(), proc_dir)
                })
            } else {
                with_dir(current_dir.clone(), |dir| dir.get_entry(component)).flatten()
            }?;
            if !child.is_dir() {
                return Err(FSError::simple(FSErrorKind::InvalidPath));
            }
            current_dir = child;
        }

        if create {
            with_dir(current_dir, |dir| {
                dir.ensure_entry(path.file().into(), proc_dir)
            })
        } else {
            with_dir(current_dir, |dir| dir.get_entry(path.file())).flatten()
        }
    }
}

impl FS for ProcFS {
    fn open(
        &self,
        path: &super::Path,
        options: super::OpenOptions,
    ) -> super::FSResult<crate::kernel::fd::FileBuilder> {
        let Some(parent) = path.parent() else {
            return Ok(as_file(self.root.clone()).with_perms(options));
        };

        let create_all = options.contains(OpenOptions::CREATE_ALL);
        let parent = self.traverse(parent, create_all)?;

        let ProcNode::Dir(ref parent_dir) = parent.node else {
            return Err(FSError::simple(FSErrorKind::InvalidPath));
        };

        if path.as_str().ends_with('/') {
            Ok(as_file(parent).with_perms(options))
        } else if options.contains(OpenOptions::CREATE_DIR) {
            Ok(as_file(parent_dir.ensure_entry(path.file().into(), proc_dir)).with_perms(options))
        } else {
            let entry = parent_dir.get_or_update(path, path.file())?;
            Ok(as_file(entry).with_perms(options))
        }
    }

    fn unlink(
        &self,
        path: &super::Path,
        options: super::UnlinkOptions,
    ) -> super::FSResult<crate::kernel::fd::FileBuilder> {
        let parent = if path.as_str().ends_with('/')
            && let Some(dir) = path.parent()
            && let Some(parent) = path.parent()
        {
            if options.contains(UnlinkOptions::RECURSIVE) {
                self.traverse(parent, false)
            } else {
                Err(FSError::simple(FSErrorKind::PermissionDenied))
            }
        } else if let Some(parent) = path.parent() {
            self.traverse(parent, false)
        } else if options.contains(
            UnlinkOptions::NO_PRESERVE_ROOT | UnlinkOptions::FORCE | UnlinkOptions::RECURSIVE,
        ) {
            Ok(self.root.clone())
        } else {
            Err(FSError::simple(FSErrorKind::PermissionDenied))
        }?;

        let child = with_dir(parent.clone(), |dir| dir.get_entry(path.file())).flatten()?;

        let removed = match &child.node {
            ProcNode::Dir(_) => {
                if options.contains(UnlinkOptions::RECURSIVE) {
                    with_dir(parent, |dir| {
                        dir.inner
                            .write()
                            .swap_remove(path.file())
                            .ok_or(FSError::simple(FSErrorKind::NotFound))
                    })
                    .flatten()
                } else {
                    Err(FSError::simple(FSErrorKind::PermissionDenied))
                }
            }
            ProcNode::File(_) => with_dir(parent, |dir| {
                dir.inner
                    .write()
                    .swap_remove(path.file())
                    .ok_or(FSError::simple(FSErrorKind::NotFound))
            })
            .flatten(),
        }?;

        Ok(as_file(removed))
    }

    // updates all children of path, if path is a dir
    fn flush(&self, path: &super::Path) -> super::FSResult<()> {
        // TODO optimize this, maybe by using a Trie instead of HashMap
        let node = self.traverse(path, true)?;
        let ProcNode::Dir(ref d) = node.node else {
            return Err(FSError::simple(FSErrorKind::NotADir));
        };
        let registry = registry();
        let should_sanitize = path.as_str().ends_with('/');

        for (device_path, node) in registry.devices.read().iter() {
            if let Some(postfix) = device_path.strip_prefix(&path) {
                let postfix = if should_sanitize {
                    let Some(p) = postfix.strip_prefix(&"/") else {
                        continue;
                    };
                    p
                } else {
                    postfix
                };
                d.inner.write().insert(
                    postfix.as_str().into(),
                    proc_file(node.clone().into_inner()),
                );
            }
        }
        Ok(())
    }
}

impl FileRepr for ProcFS {
    fn fstat(&self) -> FStat {
        FStat::new()
    }

    fn node_type(&self) -> super::NodeType {
        super::NodeType::Mount
    }
}

impl IOCapable for ProcFS {}

impl Read for ProcFS {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        let bytes = "ProcFS".as_bytes();
        let len = bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }
}

impl Write for ProcFS {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        Err(FSError::simple(FSErrorKind::NotSupported))
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use alloc::{format, vec};

    use os_macros::kernel_test;

    use super::*;
    use crate::kernel::fs::NodeType;

    #[kernel_test]
    fn procfs_basic() {
        let procfs = ProcFS::new();
        assert!(
            procfs
                .open(Path::new("/foo/bar"), OpenOptions::default())
                .is_err()
        );
        assert!(
            procfs
                .open(
                    Path::new("/foo"),
                    OpenOptions::CREATE_DIR | OpenOptions::READ
                )
                .is_ok()
        );
        assert!(
            procfs
                .open(
                    Path::new("/foobar/bar/baz.txt"),
                    OpenOptions::CREATE_ALL | OpenOptions::READ
                )
                .is_err()
        );
        assert!(
            procfs
                .open(
                    Path::new("/foobar/bar/baz/"),
                    OpenOptions::CREATE_ALL | OpenOptions::READ
                )
                .is_ok()
        );
        assert!(
            procfs
                .open(Path::new("/foobar/bar"), OpenOptions::READ)
                .is_ok()
        );
        assert!(
            procfs
                .unlink(
                    Path::new("/foobar"),
                    UnlinkOptions::RECURSIVE | UnlinkOptions::FORCE
                )
                .is_ok()
        );
        assert!(
            procfs
                .open(Path::new("/foobar/bar/baz.txt"), OpenOptions::READ)
                .is_err()
        );
        assert!(
            procfs
                .unlink(Path::new("/"), UnlinkOptions::RECURSIVE)
                .is_err()
        );
    }

    #[kernel_test]
    fn test_rw() {
        let procfs = ProcFS::new();

        let registry = DEVICE_REGISTRY.get_or_init(|| DeviceRegistry::new());

        #[derive(Debug)]
        struct TestDevice;

        impl FileRepr for TestDevice {
            fn fstat(&self) -> FStat {
                FStat::new()
            }

            fn node_type(&self) -> NodeType {
                NodeType::File
            }
        }

        impl IOCapable for TestDevice {}

        impl Read for TestDevice {
            fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
                let bytes = "Test Device".as_bytes();
                let len = bytes.len().min(buf.len());
                buf[..len].copy_from_slice(&bytes[..len]);
                Ok(len)
            }
        }

        impl Write for TestDevice {
            fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
                Err(FSError::simple(FSErrorKind::NotSupported))
            }
        }

        let test_device = Arc::new(TestDevice);

        assert!(
            procfs
                .open(Path::new("/Test.dev"), OpenOptions::READ)
                .is_err()
        );
        assert!(
            registry
                .register(test_device.clone(), Path::new("/Test.dev").into())
                .is_ok()
        );

        let mut file = procfs
            .open(
                Path::new("/Test.dev"),
                OpenOptions::READ | OpenOptions::WRITE,
            )
            .unwrap()
            .finish();
        let mut buf = vec![0; 50];

        let n = file.read_continuous(&mut buf).unwrap();
        assert_eq!(n, "Test Device".as_bytes().len());

        assert_eq!(str::from_utf8(&buf[..n]).unwrap(), "Test Device");
        assert!(file.write_continuous("Hello".as_bytes()).is_err());

        assert!(
            procfs
                .unlink(Path::new("/Test.dev"), UnlinkOptions::empty())
                .is_ok()
        );
        assert!(
            procfs
                .open(Path::new("/Test.dev"), OpenOptions::READ)
                .is_ok()
        );

        assert!(registry.deregister(Path::new("/Test.dev")).is_ok());
        assert!(
            procfs
                .unlink(Path::new("/Test.dev"), UnlinkOptions::empty())
                .is_ok()
        );
        assert!(
            procfs
                .open(Path::new("/Test.dev"), OpenOptions::READ)
                .is_err()
        );
    }

    #[kernel_test]
    fn read_dir() {
        let dir = proc_dir();
        with_dir(dir.clone(), |inner| assert_eq!(format!("{}", inner), "")).unwrap();
        with_dir(dir.clone(), |inner| {
            inner.ensure_entry("foo".into(), || proc_dir());
            inner.ensure_entry("foobar".into(), || empty_proc_file());
            inner.ensure_entry("this is a veeery long directory name!!".into(), || {
                proc_dir()
            });
            inner.ensure_entry("short".into(), || proc_dir());
        })
        .unwrap();
        let display = with_dir(dir, |inner| format!("{}", inner)).unwrap();

        let expected = "foo\tfoobar\tthis is a veeery long directory name!!\tshort\t";

        // for some unknown reason str::PartialEq comparison causes UB in this case.
        // Thus we compare the bytes manually
        // TODO:  FIX THIS

        let display_bytes = display.as_bytes();
        let expected_bytes = expected.as_bytes();

        assert_eq!(display_bytes.len(), expected_bytes.len());

        for (i, (&a, &b)) in display_bytes.iter().zip(expected_bytes.iter()).enumerate() {
            if a != b {
                panic!("Diff at {}: {} != {}", i, a, b);
            }
        }

        // assert_eq!(display_bytes, expected_bytes);

        // assert_eq!(
        //     display.as_str(),
        //     "foo\tfoobar\tthis is a veeery long directory name!!\tshort\t"
        // )
    }
}
