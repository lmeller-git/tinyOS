use alloc::{
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};
use core::{fmt::Display, ops::Sub};

use hashbrown::DefaultHashBuilder;
use indexmap::IndexMap;
use thiserror::Error;

use crate::{
    kernel::{
        fd::{FStat, File, FileRepr, IOCapable},
        fs::{
            FS,
            FSError,
            FSErrorKind,
            FSResult,
            OpenOptions,
            Path,
            PathBuf,
            UnlinkOptions,
            fs_util::open,
        },
        io::{Read, Write},
    },
    sync::locks::RwLock,
};

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

fn empty_ram_link() -> RamFilePtr {
    ram_link(PathBuf::new())
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

    fn buffered_display(&self, buf: &mut [u8], offset: usize) -> (usize, bool) {
        let mut written = 0;
        let mut newly_written = 0;
        for name in self.inner.keys() {
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
#[derive(Debug, Default)]
struct FileData {
    inner: Vec<u8>,
}

impl FileRepr for LockedRamFile {
    fn fstat(&self) -> FStat {
        self.read().stat.clone()
    }

    fn node_type(&self) -> super::NodeType {
        match self.read().node {
            RamNode::SoftLink(_) => super::NodeType::SymLink,
            RamNode::Dir(_) => super::NodeType::Dir,
            RamNode::File(_) => super::NodeType::File,
        }
    }
}

impl IOCapable for LockedRamFile {}

impl Read for LockedRamFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        match self.read().node {
            RamNode::SoftLink(ref l) => {
                let bytes = l.as_str().as_bytes();
                if offset > bytes.len() {
                    return Err(FSError::simple(FSErrorKind::UnexpectedEOF));
                }
                let n_readable = (bytes.len() - offset).min(buf.len());
                buf[..n_readable].copy_from_slice(&bytes[offset..n_readable + offset]);
                Ok(n_readable)
            }
            RamNode::Dir(ref d) => {
                let (written, _at_end) = d.buffered_display(buf, offset);
                Ok(written)
            }
            RamNode::File(ref f) => {
                // bail early if we are at end
                if offset == f.inner.len() {
                    return Ok(0);
                }
                let len = f
                    .inner
                    .len()
                    .checked_sub(offset)
                    .ok_or(FSError::simple(FSErrorKind::UnexpectedEOF))?
                    .min(buf.len());
                buf[..len].copy_from_slice(&f.inner[offset..offset + len]);
                Ok(len)
            }
        }
    }

    fn read_to_end(
        &self,
        buf: &mut Vec<u8>,
        mut offset: usize,
    ) -> crate::kernel::io::IOResult<usize> {
        let reader = self.read();
        match reader.node {
            RamNode::SoftLink(ref l) => {
                let bytes = l.as_str().as_bytes();
                if offset > bytes.len() {
                    return Err(FSError::simple(FSErrorKind::UnexpectedEOF));
                }
                buf.extend_from_slice(&bytes[offset..]);
                Ok(bytes.len() - offset)
            }
            RamNode::Dir(ref d) => {
                let res = format!("{}", d);
                let bytes = res.as_bytes();
                buf.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            RamNode::File(ref f) => {
                let mut written = 0;
                loop {
                    let count = Read::read(self, &mut buf[written..], offset)?;
                    if count == buf[written..].len() {
                        buf.resize(buf.len().max(1) * 2, 0);
                    } else if count == 0 {
                        return Ok(written);
                    }
                    written += count;
                    offset += count;
                }
            }
        }
    }
}

impl Write for LockedRamFile {
    fn write(&self, buf: &[u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        match self.write().node {
            RamNode::SoftLink(ref mut l) => {
                let str_ =
                    str::from_utf8(buf).map_err(|_| FSError::simple(FSErrorKind::InvalidPath))?;
                let path = Path::new(str_).into();
                *l = path;
                Ok(buf.len())
            }
            RamNode::Dir(ref d) => Err(FSError::simple(FSErrorKind::NotSupported)),
            RamNode::File(ref mut f) => {
                // this currently allows to write BELOW end, leaving a 0 initialized region
                // might want to prohibit this
                if offset + buf.len() > f.inner.len() {
                    f.inner.resize(offset + buf.len(), 0);
                }
                // no need to validate offset, as we just resized
                let len = f.inner.len().sub(offset).min(buf.len());
                f.inner[offset..offset + len].copy_from_slice(&buf[..len]);
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
        } else if options.contains(OpenOptions::CREATE_LINK) {
            Ok(
                as_file(entries.ensure_entry(path.file().into(), empty_ram_link))
                    .with_perms(options),
            )
        } else if create_all || options.contains(OpenOptions::CREATE) {
            Ok(as_file(entries.ensure_entry(path.file().into(), ram_file)).with_perms(options))
        } else {
            let entry = entries
                .inner
                .get(path.file())
                .ok_or(FSError::simple(FSErrorKind::NotFound))?;
            if !options.contains(OpenOptions::NO_FOLLOW_LINK)
                && let RamNode::SoftLink(ref p) = entry.read_arc().node
            {
                open(p, options)
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

    #[kernel_test]
    fn read_dir() {
        let dir = ram_dir();
        with_dir(dir.clone(), |inner| assert_eq!(format!("{}", inner), "")).unwrap();
        with_mut_dir(dir.clone(), |inner| {
            inner.ensure_entry("foo".into(), || ram_dir());
            inner.ensure_entry("foobar".into(), || ram_file());
            inner.ensure_entry("this is a veeery long directory name!!".into(), || {
                ram_dir()
            });
            inner.ensure_entry("short".into(), || ram_dir());
        })
        .unwrap();
        let mut buf = String::new();
        let display = Read::read_to_string(dir.as_ref(), &mut buf, 0).unwrap();

        let expected = "foo\tfoobar\tthis is a veeery long directory name!!\tshort\t";

        // for some unknown reason str::PartialEq comparison causes UB in this case.
        // Thus we compare the bytes manually
        // TODO:  FIX THIS

        let display_bytes = &buf[..display].as_bytes();
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
