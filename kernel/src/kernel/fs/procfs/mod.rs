use crate::kernel::fs::FS;

#[derive(Debug)]
pub struct ProcFS {}

impl ProcFS {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FS for ProcFS {
    fn open(&self, path: &super::Path) -> super::FSResult<super::FSNode> {
        todo!()
    }

    fn close(&self, path: &super::Path) -> super::FSResult<()> {
        todo!()
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
        Self {}
    }
}
