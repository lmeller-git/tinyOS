use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use core::fmt::Debug;

use thiserror::Error;

use crate::{
    kernel::{
        fd::IOCapable,
        fs::{Dir, FS, FSError, FSErrorKind, FSNode, FSResult, File, Path, PathBuf},
    },
    sync::locks::RwLock,
};

#[derive(Error, Debug)]
pub enum ProcFSError {
    #[error("the node already exists. {}", msg)]
    NodeExists { node: FSNode, msg: &'static str },
}

#[derive(Debug)]
enum ProcFSNode {
    Dir(Arc<ProcFSDir>),
    DeviceNode(Arc<dyn File>),
    Link(PathBuf),
}

impl TryFrom<ProcFSNode> for FSNode {
    type Error = FSError;

    fn try_from(value: ProcFSNode) -> Result<Self, Self::Error> {
        match value {
            ProcFSNode::Dir(dir) => Ok(Self::Dir(dir)),
            ProcFSNode::Link(_) => Err(FSError::with_message(
                FSErrorKind::Other,
                "links are not yet supported",
            )),
            ProcFSNode::DeviceNode(device) => todo!(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ProcFSDir {
    children: RwLock<BTreeMap<PathBuf, ProcFSNode>>,
}

impl ProcFSDir {
    pub fn new() -> Self {
        Self {
            children: RwLock::default(),
        }
    }
}

impl Dir for ProcFSDir {}

#[derive(Debug)]
pub struct ProcFS {
    root: Arc<ProcFSDir>,
}

impl ProcFS {
    pub fn new() -> Self {
        Self::default()
    }

    fn traverse(&self, path: &Path) -> FSResult<ProcFSNode> {
        let mut current_dir = self.root.clone();
        for component in path.traverse().skip(1) {
            let reader = current_dir.children.read();
            match reader.get(Path::new(component)) {
                Some(ProcFSNode::Dir(d)) => {
                    if component == path.file() {
                        return Ok(ProcFSNode::Dir(d.clone()));
                    }
                    let node = d.clone();
                    drop(reader);
                    current_dir = node;
                }
                Some(ProcFSNode::DeviceNode(f)) => {
                    if component == path.file() {
                        return Ok(ProcFSNode::DeviceNode(f.clone()));
                    }
                    return Err(FSError::simple(FSErrorKind::NotADir));
                }
                Some(ProcFSNode::Link(l)) => {
                    // TODO: follow link? (link to dir?)
                    return Err(FSError::with_message(
                        FSErrorKind::Other,
                        "Links are not yet supported",
                    ));
                }
                None => return Err(FSError::simple(FSErrorKind::NotFound)),
            }
        }
        Ok(ProcFSNode::Dir(self.root.clone()))
    }
}

impl FS for ProcFS {
    fn open(&self, path: &super::Path) -> super::FSResult<super::FSNode> {
        self.traverse(path)?.try_into()
    }

    fn close(&self, path: &super::Path) -> super::FSResult<()> {
        Err(FSError::with_message(
            FSErrorKind::Other,
            "currently cannot close nodes",
        ))
    }

    fn add_node(&self, path: &super::Path, node: super::FSNode) -> super::FSResult<()> {
        todo!()
    }

    fn remove_node(&self, path: &super::Path) -> super::FSResult<super::FSNode> {
        todo!()
    }
}

impl Default for ProcFS {
    fn default() -> Self {
        Self {
            root: Arc::default(),
        }
    }
}
