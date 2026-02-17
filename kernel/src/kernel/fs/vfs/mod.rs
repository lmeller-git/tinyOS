use alloc::{collections::btree_map::BTreeMap, string::String, sync::Arc};
use core::fmt::Display;

use conquer_once::spin::OnceCell;
use hashbrown::DefaultHashBuilder;
use indexmap::IndexMap;
use thiserror::Error;

use crate::{
    kernel::{
        fd::{FStat, FileBuilder, FileRepr, IOCapable, MaybeOwned},
        fs::{FS, FSError, FSErrorKind, FSResult, OpenOptions, Path, PathBuf, UnlinkOptions},
        io::{Read, Write},
    },
    serial_println,
    sync::{
        BlockingWaiter,
        locks::{GenericRwLock, RwLock},
    },
};

pub static VFS: OnceCell<Arc<VFS>> = OnceCell::uninit();

pub fn init() {
    VFS.init_once(|| VFS::new().into());
}

pub fn get() -> &'static Arc<VFS> {
    VFS.get_or_init(|| VFS::new().into())
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
    fn open(&self, path: &Path, options: OpenOptions) -> FSResult<crate::kernel::fd::FileBuilder> {
        if path == Path::new("/") {
            return Ok(FileBuilder::new(get().clone() as Arc<dyn FileRepr>)
                .with_perms(options)
                .with_path(path.into()));
        }
        self.deepest_matching_mount(path)
            .and_then(|(mount, path)| mount.open(path, options))
    }

    fn unlink(
        &self,
        path: &Path,
        options: UnlinkOptions,
    ) -> FSResult<crate::kernel::fd::FileBuilder> {
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

impl FileRepr for VFS {
    fn fstat(&self) -> crate::kernel::fd::FStat {
        FStat::default()
    }

    fn node_type(&self) -> super::NodeType {
        super::NodeType::Mount
    }
}

impl IOCapable for VFS {}

impl Read for VFS {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        let (n_read, read_to_end) = self.buffered_display(buf, offset);
        if n_read == 0 && read_to_end {
            Err(FSError::simple(FSErrorKind::UnexpectedEOF))
        } else {
            Ok(n_read)
        }
    }

    fn read_to_end(
        &self,
        buf: &mut alloc::vec::Vec<u8>,
        mut offset: usize,
    ) -> crate::kernel::io::IOResult<usize> {
        let res = alloc::format!("{}", self);
        let bytes = res.as_bytes();
        buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }
}

impl Write for VFS {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        Err(FSError::simple(FSErrorKind::NotSupported))
    }
}

impl VFS {
    // writes names for all entries in self into buffer, while buffer has space, separated by '\t'. Writes either a whole name + '\t', or nothing
    // returns (_, true) if no entries remain
    pub fn buffered_display(&self, buf: &mut [u8], offset: usize) -> (usize, bool) {
        let mut written = 0;
        let mut newly_written = 0;
        for name in self.mount_table.read().keys() {
            let bytes = name.as_str().as_bytes();
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

impl Display for VFS {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("VFS with mounts:\n")?;
        let mut buf = alloc::vec![0; 10];
        let mut offset = 0;
        loop {
            let (read, is_done) = self.buffered_display(&mut buf, offset);
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

#[cfg(feature = "test_run")]
mod tests {

    use alloc::vec;

    use os_macros::kernel_test;

    use super::*;
    use crate::kernel::{
        fd::{FStat, FileRepr, IOCapable},
        fs::{
            NodeType,
            OpenOptions,
            Path,
            mount,
            open,
            procfs::{DEVICE_REGISTRY, DeviceRegistry, ProcFS},
            ramfs::RamFS,
            symlink,
            unmount,
        },
        io::{Read, Write},
    };

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

    #[kernel_test]
    fn vfs_integration() {
        let vfs = VFS::new();
        let procfs = Arc::new(ProcFS::new());
        let ramfs = Arc::new(RamFS::new());
        let registry = DEVICE_REGISTRY.get_or_init(|| DeviceRegistry::new());

        #[derive(Debug)]
        struct TestDevice;

        impl FileRepr for TestDevice {
            fn fstat(&self) -> FStat {
                FStat::new()
            }

            fn node_type(&self) -> NodeType {
                NodeType::Void
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

        assert!(vfs.mount(Path::new("/proc").into(), procfs).is_ok());
        assert!(vfs.mount(Path::new("/ram").into(), ramfs).is_ok());

        let mut ramfile = vfs
            .open(
                Path::new("/ram/foo/bar.txt"),
                OpenOptions::CREATE_ALL | OpenOptions::WRITE,
            )
            .unwrap()
            .finish();
        // for now path to device must be rooted in proc, ie start with the proc's root, NOT with the path to proc
        assert!(
            registry
                .register(test_device, Path::new("/foo/Test.dev").into())
                .is_ok()
        );

        let mut proc_file = vfs
            .open(
                Path::new("/proc/foo/Test.dev"),
                OpenOptions::CREATE_ALL | OpenOptions::READ,
            )
            .unwrap()
            .finish();

        let writer = "Hello world!!".as_bytes();
        let mut reader = vec![0; 50];

        assert_eq!(ramfile.write_continuous(writer).unwrap(), writer.len());
        ramfile.set_cursor(0);
        let n = ramfile.read_continuous(&mut reader).unwrap();
        assert_eq!(n, writer.len());
        assert_eq!(str::from_utf8(&reader[..n]).unwrap(), "Hello world!!");
        ramfile.set_cursor(0);
        assert_eq!(
            ramfile.write_continuous("Huhu".as_bytes()).unwrap(),
            "Huhu".as_bytes().len()
        );
        let n = ramfile.read(&mut reader, 0).unwrap();
        assert_eq!(
            n,
            "Huhu world!!"
                .as_bytes()
                .len()
                .max("Hello world!!".as_bytes().len())
        );

        assert_eq!(str::from_utf8(&reader[..n]).unwrap(), "Huhuo world!!");

        let n = proc_file.read_continuous(&mut reader).unwrap();
        assert_eq!(n, "Test Device".as_bytes().len());
        assert_eq!(str::from_utf8(&reader[..n]).unwrap(), "Test Device");
    }

    #[kernel_test(verbose)]
    fn symlink_() {
        mount(
            Path::new("/ram0").into(),
            Arc::new(RamFS::new()) as Arc<dyn FS>,
        )
        .unwrap();
        mount(
            Path::new("/ram1").into(),
            Arc::new(RamFS::new()) as Arc<dyn FS>,
        )
        .unwrap();

        let mut file = open(
            Path::new("/ram0/bar/foo.txt"),
            OpenOptions::CREATE_ALL | OpenOptions::WRITE,
        )
        .unwrap();

        symlink(Path::new("/ram1/foo_link"), Path::new("/ram0/bar/foo.txt")).unwrap();

        let mut link = open(Path::new("/ram1/foo_link"), OpenOptions::WRITE).unwrap();
        assert_eq!(
            file.read_all_as_str().unwrap(),
            link.read_all_as_str().unwrap()
        );
        link.set_cursor(0);

        // for some unknown reason str::PartialEq comparison causes UB in this case.
        // Thus we compare the bytes manually
        // TODO:  FIX THIS

        fn bytely_assert(display: &str, expected: &str) {
            let display_bytes = display.as_bytes();
            let expected_bytes = expected.as_bytes();

            assert_eq!(display_bytes.len(), expected_bytes.len());

            for (i, (&a, &b)) in display_bytes.iter().zip(expected_bytes.iter()).enumerate() {
                if a != b {
                    panic!("Diff at {}: {} != {}", i, a, b);
                }
            }
        }

        let str_ = "hello world in foo";
        file.write_all(str_.as_bytes(), 0).unwrap();
        file.set_cursor(0);

        bytely_assert(&file.read_all_as_str().unwrap(), str_);
        file.set_cursor(0);

        bytely_assert(&link.read_all_as_str().unwrap(), str_);
        link.set_cursor(0);

        let str_ = "well this is a new str!";
        link.write_all(str_.as_bytes(), 0).unwrap();
        link.set_cursor(0);

        bytely_assert(&file.read_all_as_str().unwrap(), str_);

        let link2 = open(
            Path::new("/ram1/foo_link"),
            OpenOptions::READ | OpenOptions::NO_FOLLOW_LINK,
        )
        .unwrap();

        bytely_assert(&link2.read_all_as_str().unwrap(), "/ram0/bar/foo.txt");

        unmount(Path::new("/ram0")).unwrap();
        unmount(Path::new("/ram1")).unwrap();
    }
}
