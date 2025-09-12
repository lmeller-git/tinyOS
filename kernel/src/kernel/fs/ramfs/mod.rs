use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec::Vec};
use core::{fmt, sync::atomic::AtomicUsize};

use thiserror::Error;

use crate::{
    arch::x86::current_time,
    kernel::{
        fd::IOCapable,
        fs::{Dir, FMeta, FS, FSError, FSErrorKind, FSNode, FSResult, File, Path, PathBuf},
        io::{Read, Write},
    },
    sync::locks::RwLock,
};

#[derive(Error, Debug)]
pub enum RamFSError {
    #[error("the node already exists. {}", msg)]
    NodeExists { node: FSNode, msg: &'static str },
}

#[derive(Debug)]
enum RamNode {
    Dir(Arc<RamDir>),
    RFNode(Arc<RwLock<RFNode>>),
    Link(PathBuf),
}

impl TryFrom<RamNode> for FSNode {
    type Error = FSError;

    fn try_from(value: RamNode) -> Result<Self, Self::Error> {
        match value {
            RamNode::Dir(dir) => Ok(Self::Dir(dir)),
            RamNode::Link(_) => Err(FSError::with_message(
                FSErrorKind::Other,
                "links are not yet supported",
            )),
            RamNode::RFNode(file) => Ok(Self::File(Arc::new(RamFileHandle::new(file)))),
        }
    }
}

#[derive(Debug)]
pub struct RamDir {
    children: RwLock<BTreeMap<PathBuf, RamNode>>,
}

impl RamDir {
    pub fn new() -> Self {
        Self {
            children: RwLock::default(),
        }
    }
}

impl Dir for RamDir {}

#[derive(Debug)]
struct RFNode {
    data: Vec<u8>,
    meta: FMeta,
}

impl RFNode {
    fn new() -> Self {
        let now = current_time().as_secs();
        Self {
            data: Vec::new(),
            meta: FMeta {
                t_create: now,
                t_mod: now,
            },
        }
    }
}

impl Default for RFNode {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct RamFileHandle {
    node: Arc<RwLock<RFNode>>,
    cursor: AtomicUsize,
}

impl RamFileHandle {
    fn new(node: Arc<RwLock<RFNode>>) -> Self {
        Self {
            node,
            cursor: AtomicUsize::new(0),
        }
    }
}

impl File for RamFileHandle {
    fn meta(&self) -> FMeta {
        self.node.read_arc().meta.clone()
    }
}

impl IOCapable for RamFileHandle {}

impl Read for RamFileHandle {
    fn read(&mut self, buf: &mut [u8]) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

impl Write for RamFileHandle {
    fn write(&self, buf: &[u8]) -> crate::kernel::io::IOResult<usize> {
        todo!()
    }
}

#[derive(Debug)]
pub struct RamFS {
    root: Arc<RamDir>,
}

impl RamFS {
    pub fn new() -> Self {
        Self::default()
    }

    fn traverse(&self, path: &Path) -> FSResult<RamNode> {
        let mut current_dir = self.root.clone();
        for component in path.traverse().skip(1) {
            let reader = current_dir.children.read();
            match reader.get(Path::new(component)) {
                Some(RamNode::Dir(d)) => {
                    if component == path.file() {
                        return Ok(RamNode::Dir(d.clone()));
                    }
                    let node = d.clone();
                    drop(reader);
                    current_dir = node;
                }
                Some(RamNode::RFNode(f)) => {
                    if component == path.file() {
                        return Ok(RamNode::RFNode(f.clone()));
                    }
                    return Err(FSError::simple(FSErrorKind::NotADir));
                }
                Some(RamNode::Link(l)) => {
                    // TODO: follow link? (link to dir?)
                    return Err(FSError::with_message(
                        FSErrorKind::Other,
                        "Links are not yet supported",
                    ));
                }
                None => return Err(FSError::simple(FSErrorKind::NotFound)),
            }
        }
        Ok(RamNode::Dir(self.root.clone()))
    }
}

impl FS for RamFS {
    fn open(&self, path: &super::Path) -> super::FSResult<super::FSNode> {
        Ok(self.traverse(path)?.try_into()?)
    }

    fn close(&self, path: &super::Path) -> super::FSResult<()> {
        Err(FSError::with_message(
            FSErrorKind::Other,
            "currently cannot close nodes",
        ))
    }

    fn add_node(&self, path: &super::Path, node: super::FSNode) -> super::FSResult<()> {
        let parent_node = if let Some(parent) = path.parent() {
            self.traverse(parent)?
        } else {
            RamNode::Dir(self.root.clone())
        };

        let RamNode::Dir(dir) = parent_node else {
            return Err(FSError::simple(FSErrorKind::NotADir));
        };

        dir.children
            .write()
            .insert(
                path.file().into(),
                match node {
                    FSNode::Dir(_) => RamNode::Dir(Arc::new(RamDir::new())),
                    FSNode::File(_) => RamNode::RFNode(Arc::new(RwLock::default())),
                    FSNode::Link(l) => {
                        return Err(FSError::with_message(
                            FSErrorKind::Other,
                            "links not supported yet",
                        ));
                    }
                    FSNode::Fs(_) => {
                        return Err(FSError::with_message(
                            FSErrorKind::Other,
                            "cannot add File systems into RamFS",
                        ));
                    }
                },
            )
            .map_or(Ok(()), |node| {
                Err(FSError::custom(
                    FSErrorKind::AlreadyExists,
                    RamFSError::NodeExists {
                        node: node.try_into()?,
                        msg: "the old node was swapped out and returned",
                    }
                    .into(),
                ))
            })
    }

    fn remove_node(&self, path: &super::Path) -> super::FSResult<super::FSNode> {
        todo!()
    }
}

impl Default for RamFS {
    fn default() -> Self {
        Self {
            root: Arc::new(RamDir::new()),
        }
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use os_macros::kernel_test;

    use super::*;

    #[kernel_test]
    fn ramfs_basic() {
        let fs = RamFS::new();
        assert!(fs.open(Path::new("/foo/bar")).is_err());
        assert!(
            fs.add_node(
                Path::new("/foo/bar"),
                FSNode::File(Arc::new(RamFileHandle::new(Arc::new(RwLock::default()))))
            )
            .is_err()
        );

        assert!(
            fs.add_node(Path::new("/foo"), FSNode::Dir(Arc::new(RamDir::new())))
                .is_ok()
        );

        assert!(fs.open(Path::new("/foo")).is_ok());

        assert!(
            fs.add_node(
                Path::new("/foo/bar"),
                FSNode::File(Arc::new(RamFileHandle::new(Arc::new(RwLock::default()))))
            )
            .is_ok()
        );

        assert!(fs.open(&Path::new("/foo/bar"),).is_ok());
        assert!(
            fs.add_node(
                Path::new("/foo/bar/baz"),
                FSNode::Dir(Arc::new(RamDir::new()))
            )
            .is_err()
        );
        assert!(
            fs.add_node(Path::new("/foo/baz"), FSNode::Fs(Arc::new(RamFS::new())))
                .is_err()
        );
    }
}
