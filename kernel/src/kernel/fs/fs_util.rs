use alloc::{string::String, sync::Arc};

use crate::kernel::{
    fd::File,
    fs::{FS, FSResult, OpenOptions, Path, PathBuf, UnlinkOptions, fs, vfs},
    io::{Read, Write},
};

pub fn mkdir(path: &Path) -> FSResult<()> {
    fs().open(path, OpenOptions::CREATE_DIR)?;
    Ok(())
}

pub fn lsdir(path: &Path) -> FSResult<String> {
    let dir = fs().open(path, OpenOptions::READ)?;
    let mut buf = String::new();
    let res = dir.read_to_string(&mut buf, 0)?;
    buf.truncate(res);
    Ok(buf)
}

pub fn mount(path: PathBuf, fs: Arc<dyn FS>) -> FSResult<()> {
    vfs::get().mount(path, fs)
}

pub fn unmount(path: &Path) -> FSResult<()> {
    vfs::get().unmount(path)?;
    Ok(())
}

pub fn open(path: &Path, options: OpenOptions) -> FSResult<File> {
    fs().open(path, options)
        .map(|file| file.with_path(path.into()))
}

pub fn close(path: &Path, file: File) -> FSResult<()> {
    Ok(())
}

pub fn rm(path: &Path, options: UnlinkOptions) -> FSResult<()> {
    fs().unlink(path, options)?;
    Ok(())
}

pub fn symlink(path: &Path, to: &Path) -> FSResult<()> {
    let link = fs().open(path, OpenOptions::CREATE_LINK.with_write())?;
    let str_ = to.as_str();
    let bytes = str_.as_bytes();
    let res = link.write(bytes, 0)?;
    debug_assert_eq!(res, bytes.len());
    Ok(())
}
