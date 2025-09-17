use alloc::{
    string::{String, ToString},
    sync::Arc,
};
use core::ops::Deref;

use bitflags::Flags;
use hashbrown::DefaultHashBuilder;
use indexmap::IndexMap;
use thiserror::Error;

use crate::{
    kernel::{
        fd::{FStat, File, FileRepr, IOCapable},
        fs::{FS, FSError, FSErrorKind, FSResult, OpenOptions, Path, UnlinkOptions},
        io::{Read, Write},
    },
    sync::locks::RwLock,
};

const NULL_DEVICE: &'static Null = &Null;

#[derive(Error, Debug)]
pub enum ProcFSError {}

type ProcFilePtr = Arc<ProcFile>;

#[derive(Debug)]
struct ProcFile {
    node: ProcNode,
    stat: RwLock<FStat>,
}

impl ProcFile {
    pub fn new(node: ProcNode) -> Self {
        Self {
            node,
            stat: RwLock::new(FStat::new()),
        }
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
        todo!()
    }
}

impl IOCapable for ProcFile {}

impl Read for ProcFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl Write for ProcFile {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

#[derive(Debug)]
enum ProcNode {
    Dir(DirData),
    File(MaybeRefCounted<'static>), // a File, typically a device, living for 'static or owned by arc
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
}

#[derive(Debug)]
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
}

#[derive(Debug, Default)]
pub struct Null;

impl FileRepr for Null {
    fn fstat(&self) -> FStat {
        FStat::new()
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

fn as_file(ptr: ProcFilePtr) -> File {
    File::new(ptr as Arc<dyn FileRepr>)
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
    ) -> super::FSResult<crate::kernel::fd::File> {
        let Some(parent) = path.parent() else {
            return Ok(as_file(self.root.clone()).with_perms(options));
        };

        let create_all = options.contains(OpenOptions::CREATE_ALL);
        let parent = self.traverse(parent, create_all)?;

        let ProcNode::Dir(ref parent_dir) = parent.node else {
            return Err(FSError::simple(FSErrorKind::InvalidPath));
        };

        if (create_all || options.contains(OpenOptions::CREATE_DIR)) && path.as_str().ends_with('/')
        {
            Ok(as_file(parent).with_perms(options))
        } else if options.contains(OpenOptions::CREATE_DIR) {
            Ok(as_file(parent_dir.ensure_entry(path.file().into(), proc_dir)).with_perms(options))
        } else if create_all || options.contains(OpenOptions::CREATE) {
            Ok(
                as_file(parent_dir.ensure_entry(path.file().into(), empty_proc_file))
                    .with_perms(options),
            )
        } else if path.as_str().ends_with('/') {
            Ok(as_file(parent).with_perms(options))
        } else {
            let entry = parent_dir.get_entry(path.file())?;
            Ok(as_file(entry).with_perms(options))
        }
    }

    fn unlink(
        &self,
        path: &super::Path,
        options: super::UnlinkOptions,
    ) -> super::FSResult<crate::kernel::fd::File> {
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

    fn flush(&self, path: &super::Path) -> super::FSResult<()> {
        // nothing to do
        Ok(())
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use os_macros::kernel_test;

    use super::*;

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
}
