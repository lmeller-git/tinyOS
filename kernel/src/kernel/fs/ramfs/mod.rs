use thiserror::Error;

#[derive(Error, Debug)]
pub enum RamFSError {}

#[cfg(feature = "test_run")]
mod tests {
    use os_macros::kernel_test;

    use super::*;

    #[kernel_test]
    fn ramfs_basic() {}
}
