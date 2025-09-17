use alloc::{
    collections::btree_map::BTreeMap,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::ptr;

use conquer_once::spin::OnceCell;
use hashbrown::DefaultHashBuilder;
use indexmap::IndexMap;
use thiserror::Error;

use crate::{
    kernel::{
        fd::{FStat, File, FileRepr, IOCapable},
        fs::{FS, FSError, FSErrorKind, FSResult, OpenOptions, Path, PathBuf, UnlinkOptions},
        io::{Read, Write},
    },
    sync::locks::RwLock,
};

pub static RAMFS: OnceCell<RamFS> = OnceCell::uninit();

pub fn init() {
    RAMFS.init_once(|| RamFS::new());
}

#[derive(Error, Debug)]
pub enum RamFSError {}

pub type LockedRamFile = RwLock<RamFile>;
pub type RamFilePtr = Arc<LockedRamFile>;

#[derive(Debug)]
struct RamFile {
    stat: FStat,
    node: RamNode,
}

impl RamFile {
    fn new(node: RamNode) -> Self {
        Self {
            stat: FStat::new(),
            node,
        }
    }

    fn is_dir(&self) -> bool {
        match self.node {
            RamNode::Dir(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
enum RamNode {
    SoftLink(PathBuf),
    File(FileData),
    Dir(DirData),
}

impl RamNode {
    fn file() -> Self {
        Self::File(FileData::default())
    }

    fn dir() -> Self {
        Self::Dir(DirData::default())
    }

    fn link(path: PathBuf) -> Self {
        Self::SoftLink(path)
    }
}

fn ram_dir() -> RamFilePtr {
    RamFilePtr::new(LockedRamFile::new(RamFile::new(RamNode::dir())))
}

fn ram_file() -> RamFilePtr {
    RamFilePtr::new(LockedRamFile::new(RamFile::new(RamNode::file())))
}

fn ram_link(path: PathBuf) -> RamFilePtr {
    RamFilePtr::new(LockedRamFile::new(RamFile::new(RamNode::link(path))))
}

fn as_file(ptr: RamFilePtr) -> File {
    File::new(ptr as Arc<dyn FileRepr>)
}

fn with_mut_dir<F, R>(parent: RamFilePtr, func: F) -> FSResult<R>
where
    F: FnOnce(&mut DirData) -> R,
{
    let RamNode::Dir(ref mut d) = parent.write_arc().node else {
        return Err(FSError::simple(FSErrorKind::InvalidPath));
    };
    Ok(func(d))
}

fn with_dir<F, R>(parent: RamFilePtr, func: F) -> FSResult<R>
where
    F: FnOnce(&DirData) -> R,
{
    let RamNode::Dir(ref d) = parent.read_arc().node else {
        return Err(FSError::simple(FSErrorKind::InvalidPath));
    };
    Ok(func(d))
}

#[derive(Debug, Default)]
struct DirData {
    inner: IndexMap<String, RamFilePtr, DefaultHashBuilder>,
}

impl DirData {
    fn ensure_entry<F>(&mut self, name: String, f: F) -> RamFilePtr
    where
        F: FnOnce() -> RamFilePtr,
    {
        self.inner.entry(name).or_insert_with(f).clone()
    }
}

#[derive(Debug, Default)]
struct FileData {
    inner: Vec<u8>,
}

impl FileRepr for LockedRamFile {
    fn fstat(&self) -> FStat {
        self.read().stat.clone()
    }
}

impl IOCapable for LockedRamFile {}

impl Read for LockedRamFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        match self.read().node {
            RamNode::SoftLink(ref l) => Err(FSError::simple(FSErrorKind::NotSupported)),
            RamNode::Dir(ref d) => Err(FSError::simple(FSErrorKind::NotSupported)),
            RamNode::File(ref f) => {
                // bail early if we are at end
                if offset == f.inner.len() {
                    return Ok(0);
                }
                let len = f
                    .inner
                    .len()
                    .checked_sub(offset)
                    .ok_or(FSError::simple(FSErrorKind::UnexpectedEOF))?;
                unsafe {
                    ptr::copy_nonoverlapping(f.inner[offset..].as_ptr(), buf.as_mut_ptr(), len);
                }

                Ok(len)
            }
        }
    }
}

impl Write for LockedRamFile {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        match self.write().node {
            RamNode::SoftLink(ref l) => Err(FSError::simple(FSErrorKind::NotSupported)),
            RamNode::Dir(ref d) => Err(FSError::simple(FSErrorKind::NotSupported)),
            RamNode::File(ref mut f) => {
                // this currently allows to write BELOW end, leaving a 0 initialized region
                // might want to prohibit this
                if offset + buf.len() > f.inner.len() {
                    f.inner.resize(offset + buf.len(), 0);
                }
                // in principle no need to check here
                let len = f
                    .inner
                    .len()
                    .checked_sub(offset)
                    .ok_or(FSError::simple(FSErrorKind::UnexpectedEOF))?;
                unsafe {
                    ptr::copy_nonoverlapping(buf.as_ptr(), f.inner[offset..].as_mut_ptr(), len);
                }

                Ok(len)
            }
        }
    }
}

#[derive(Debug)]
pub struct RamFS {
    root: RamFilePtr,
}

impl RamFS {
    pub fn new() -> Self {
        Self {
            root: RamFilePtr::new(RwLock::new(RamFile::new(RamNode::dir()))),
        }
    }

    fn traverse(&self, path: &Path, create: bool) -> FSResult<RamFilePtr> {
        let mut current_dir = self.root.clone();
        // skip last (target) component
        let Some(parent) = path.parent() else {
            return Ok(current_dir);
        };

        // skip root dir
        for component in parent.traverse().skip(1) {
            let child = if create {
                with_mut_dir(current_dir, |dir| {
                    dir.ensure_entry(component.to_string(), ram_dir)
                })
            } else {
                with_dir(current_dir, |dir| {
                    dir.inner
                        .get(component)
                        .cloned()
                        .ok_or(FSError::simple(FSErrorKind::NotFound))
                })?
            }?;

            if !child.read().is_dir() {
                return Err(FSError::simple(FSErrorKind::InvalidPath));
            }
            current_dir = child;
        }

        if create {
            with_mut_dir(current_dir, |dir| {
                dir.ensure_entry(path.file().into(), ram_dir)
            })
        } else {
            with_dir(current_dir, |dir| {
                dir.inner
                    .get(path.file())
                    .cloned()
                    .ok_or(FSError::simple(FSErrorKind::NotFound))
            })
            .flatten()
        }
    }
}

impl FS for RamFS {
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

        let RamNode::Dir(ref mut entries) = parent.write_arc().node else {
            return Err(FSError::simple(FSErrorKind::InvalidPath));
        };
        if path.as_str().ends_with('/') {
            Ok(as_file(parent).with_perms(options))
        } else if options.contains(OpenOptions::CREATE_DIR) {
            Ok(as_file(entries.ensure_entry(path.file().into(), ram_dir)).with_perms(options))
        } else if create_all || options.contains(OpenOptions::CREATE) {
            Ok(as_file(entries.ensure_entry(path.file().into(), ram_file)).with_perms(options))
        } else if path.as_str().ends_with('/') {
            Ok(as_file(parent).with_perms(options))
        } else {
            let entry = entries
                .inner
                .get(path.file())
                .ok_or(FSError::simple(FSErrorKind::NotFound))?;
            if !options.contains(OpenOptions::NO_FOLLOW_LINK)
                && let RamNode::SoftLink(ref p) = entry.read_arc().node
            {
                Err(FSError::simple(FSErrorKind::NotSupported))
            } else {
                Ok(as_file(entry.clone()).with_perms(options))
            }
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

        let child = with_dir(parent.clone(), |nodes| {
            nodes
                .inner
                .get(path.file())
                .cloned()
                .ok_or(FSError::simple(FSErrorKind::NotFound))
        })
        .flatten()?;

        let removed = match &child.read_arc().node {
            RamNode::Dir(_) => {
                if options.contains(UnlinkOptions::RECURSIVE) {
                    with_mut_dir(parent, |entries| {
                        entries
                            .inner
                            .swap_remove(path.file())
                            .ok_or(FSError::simple(FSErrorKind::NotFound))
                    })
                    .flatten()
                } else {
                    Err(FSError::simple(FSErrorKind::PermissionDenied))
                }
            }
            RamNode::SoftLink(_) | RamNode::File(_) => with_mut_dir(parent, |entries| {
                entries
                    .inner
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
    use alloc::vec;
    use core::{fmt::Write, ops::AddAssign};

    use os_macros::kernel_test;

    use super::*;

    #[kernel_test]
    fn ramfs_basic() {
        let ramfs = RamFS::new();
        assert!(
            ramfs
                .open(Path::new("/foo/bar"), OpenOptions::default())
                .is_err()
        );
        assert!(
            ramfs
                .open(
                    Path::new("/foo"),
                    OpenOptions::CREATE_DIR | OpenOptions::READ
                )
                .is_ok()
        );
        assert!(
            ramfs
                .open(
                    Path::new("/foobar/bar/baz.txt"),
                    OpenOptions::CREATE_ALL | OpenOptions::READ
                )
                .is_ok()
        );
        assert!(
            ramfs
                .open(Path::new("/foobar/bar"), OpenOptions::READ)
                .is_ok()
        );
        assert!(
            ramfs
                .unlink(
                    Path::new("/foobar"),
                    UnlinkOptions::RECURSIVE | UnlinkOptions::FORCE
                )
                .is_ok()
        );
        assert!(
            ramfs
                .open(Path::new("/foobar/bar/baz.txt"), OpenOptions::READ)
                .is_err()
        );
        assert!(
            ramfs
                .unlink(Path::new("/"), UnlinkOptions::RECURSIVE)
                .is_err()
        );
    }

    #[kernel_test]
    fn ramfs_retrieval() {
        let ramfs = RamFS::new();
        let mut bar = ramfs
            .open(
                Path::new("/foo/bar.txt"),
                OpenOptions::CREATE_ALL | OpenOptions::WRITE,
            )
            .unwrap();
        assert_eq!(
            bar.write_continuous("hello world".as_bytes()).unwrap(),
            "hello world".as_bytes().len()
        );
        bar.set_cursor(0);

        let mut buf = vec![0; 30];
        let n_read = bar.read_continuous(&mut buf).unwrap();
        assert_eq!(n_read, "hello_world".as_bytes().len());

        assert_eq!(str::from_utf8(&buf[..n_read]).unwrap(), "hello world");

        assert_eq!(bar.read_continuous(&mut buf[n_read..]).unwrap(), 0);
        let mut foobar = ramfs
            .open(
                Path::new("/foo/foobar"),
                OpenOptions::CREATE | OpenOptions::READ,
            )
            .unwrap();
        assert!(
            foobar
                .write_continuous("hello world/n/they".as_bytes())
                .is_err()
        );
        assert_eq!(foobar.read_continuous(&mut buf).unwrap(), 0)
    }
}
