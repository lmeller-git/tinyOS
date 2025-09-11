use alloc::{collections::btree_map::BTreeMap, sync::Arc};

use crate::{
    kernel::fs::{FS, FSResult, PathBuf},
    sync::{BlockingWaiter, locks::GenericRwLock},
};

#[derive(Debug)]
pub struct VFS {
    mount_table: GenericRwLock<BTreeMap<PathBuf, Arc<dyn FS>>, BlockingWaiter>,
}

impl VFS {
    pub fn new() -> Self {
        Self::default()
    }

    fn deepest_matching_mount(&self) -> FSResult<Arc<dyn FS>> {
        todo!()
    }

    pub fn mount(&self, fs: Arc<dyn FS>) -> FSResult<()> {
        todo!()
    }
}

impl FS for VFS {
    fn open(&self, path: &super::Path) -> FSResult<super::FSNode> {
        todo!()
    }

    fn close(&self, path: &super::Path) -> FSResult<()> {
        todo!()
    }

    fn add_node(&self, path: &super::Path, node: super::FSNode) -> FSResult<()> {
        todo!()
    }

    fn remove_node(&self, path: &super::Path) -> FSResult<super::FSNode> {
        todo!()
    }
}

impl Default for VFS {
    fn default() -> Self {
        Self {
            mount_table: GenericRwLock::default(),
        }
    }
}
