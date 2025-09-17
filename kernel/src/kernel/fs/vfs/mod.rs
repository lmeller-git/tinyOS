use alloc::{collections::btree_map::BTreeMap, sync::Arc};

use conquer_once::spin::{Once, OnceCell};
use thiserror::Error;

use crate::{
    kernel::{
        fd::MaybeOwned,
        fs::{FS, FSError, FSErrorKind, FSResult, OpenOptions, Path, PathBuf, UnlinkOptions},
    },
    sync::{BlockingWaiter, locks::GenericRwLock},
};

pub static VFS: OnceCell<VFS> = OnceCell::uninit();

pub fn init() {
    VFS.init_once(|| VFS::new());
}

#[derive(Error, Debug)]
pub enum VFSError {
    #[error("the mount already exists. {}", msg)]
    MountExists {
        mount: MaybeOwned<dyn FS>,
        msg: &'static str,
    },
}

#[derive(Debug)]
pub struct VFS {
    mount_table: GenericRwLock<BTreeMap<PathBuf, Arc<dyn FS>>, BlockingWaiter>,
}

impl VFS {
    pub fn new() -> Self {
        Self::default()
    }

    fn deepest_matching_mount<'a>(&self, path: &'a Path) -> FSResult<(Arc<dyn FS>, &'a Path)> {
        let mut target_fs = None;
        let mut postfix_path = path;
        let reader = self.mount_table.read();

        for ancestor in path.ancestors() {
            if let Some(mount) = reader.get(ancestor) {
                target_fs.replace(mount.clone());
                postfix_path = path
                    .strip_prefix(&ancestor)
                    .unwrap_or_else(|| unreachable!());
                break;
            }
        }

        target_fs
            .map(|mount| (mount, postfix_path))
            .ok_or(FSError::with_message(
                FSErrorKind::NotFound,
                "provided path matches no mount",
            ))
    }

    pub fn mount(&self, mount_point: PathBuf, fs: Arc<dyn FS>) -> FSResult<()> {
        self.mount_table
            .write()
            .insert(mount_point, fs)
            .map_or(Ok(()), |node| {
                Err(FSError::custom(
                    FSErrorKind::AlreadyExists,
                    VFSError::MountExists {
                        mount: node.into(),
                        msg: "The old mount was swapped out and returned",
                    }
                    .into(),
                ))
            })
    }

    pub fn unmount(&self, mount_point: &Path) -> FSResult<Arc<dyn FS>> {
        self.mount_table
            .write()
            .remove(mount_point)
            .ok_or(FSError::with_message(
                FSErrorKind::NotFound,
                "the mount deos not exist",
            ))
    }
}

impl FS for VFS {
    fn open(&self, path: &Path, options: OpenOptions) -> FSResult<crate::kernel::fd::File> {
        self.deepest_matching_mount(path)
            .and_then(|(mount, path)| mount.open(path, options))
    }

    fn unlink(&self, path: &Path, options: UnlinkOptions) -> FSResult<crate::kernel::fd::File> {
        self.deepest_matching_mount(path)
            .and_then(|(mount, path)| mount.unlink(path, options))
    }

    fn flush(&self, path: &Path) -> FSResult<()> {
        self.deepest_matching_mount(path)
            .and_then(|(mount, path)| mount.flush(path))
    }
}

impl Default for VFS {
    fn default() -> Self {
        Self {
            mount_table: GenericRwLock::default(),
        }
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use os_macros::kernel_test;

    use super::*;
    use crate::kernel::fs::ramfs::RamFS;

    #[kernel_test]
    fn vfs_basic() {
        let vfs = VFS::new();
        assert!(
            vfs.open(&Path::new("/foo/bar"), OpenOptions::default())
                .is_err()
        );
        assert!(vfs.unmount(&Path::new("/foo/bar")).is_err());

        let ramfs = Arc::new(RamFS::new());
        assert!(vfs.mount(Path::new("/foo").into(), ramfs).is_ok());
        assert!(
            vfs.open(
                Path::new("/foo/bar"),
                OpenOptions::CREATE | OpenOptions::READ
            )
            .is_ok()
        );
        assert!(
            vfs.open(
                Path::new("/foo_/bar"),
                OpenOptions::CREATE | OpenOptions::READ
            )
            .is_err()
        );
        assert!(
            vfs.open(Path::new("/foo/bar"), OpenOptions::default())
                .is_ok()
        );
        assert!(vfs.unmount(Path::new("/foo")).is_ok());
        assert!(
            vfs.open(Path::new("/foo/bar"), OpenOptions::default())
                .is_err()
        );
    }
}
