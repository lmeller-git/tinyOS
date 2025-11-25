use alloc::{boxed::Box, collections::btree_map::BTreeMap, sync::Arc};
use core::sync::atomic::{AtomicUsize, Ordering};

use conquer_once::spin::OnceCell;
use crossbeam::queue::ArrayQueue;

use super::TTYSource;
use crate::{
    drivers::{
        keyboard::{KEYBOARD_BUFFER, STDIN_QUEUE_SIZE, parse_scancode},
        tty::map_key,
    },
    impl_empty_write,
    impl_file_for_wr,
    impl_read_for_tty,
    kernel::{
        devices::tty::TTYSink,
        fd::{FileRepr, FileReprFactory},
        fs::{FSError, NodeType},
        threading::{
            task::{ProcessID, TaskRepr},
            tls,
        },
    },
    register_device_file,
    serial_println,
    sync::locks::RwLock,
};

pub static KEYBOARDBACKEND: OnceCell<Arc<KeyboardBackend>> = OnceCell::uninit();

pub static STDIN_FILE_FACTORY_FILE: OnceCell<Arc<StdInFileFactory>> = OnceCell::uninit();

pub fn init_source_tty() {
    KEYBOARDBACKEND.init_once(KeyboardBackend::new);
    register_device_file!(
        KEYBOARDBACKEND.get().unwrap().clone(),
        "/kernel/io/keyboard"
    );

    STDIN_FILE_FACTORY_FILE.init_once(|| Arc::new(StdInFileFactory::new()));
    register_device_file!(
        STDIN_FILE_FACTORY_FILE.get().unwrap().clone(),
        "/kernel/io/stateful_keyboard"
    );
}

// TODO cleanup open_files once process exits
// this is most easily done once process hooks are implemented

#[derive(Debug, Default)]
pub struct StdInFileFactory {
    open_files: RwLock<BTreeMap<ProcessID, OwnedStdin>>,
}

impl StdInFileFactory {
    fn new() -> Self {
        Self {
            open_files: RwLock::default(),
        }
    }

    fn ensure_init(&self, pid: ProcessID) {
        if self.open_files.read().contains_key(&pid) {
            return;
        }
        self.open_files.write().insert(pid, OwnedStdin::new());
    }

    fn delegate<F, T>(&self, pid: &ProcessID, mut callback: F) -> Option<T>
    where
        F: FnMut(&OwnedStdin) -> T,
    {
        let lock = self.open_files.read();
        let stdin = lock.get(pid)?;
        Some(callback(stdin))
    }
}

impl FileReprFactory for StdInFileFactory {
    fn get_file_impl(
        &self,
    ) -> Result<alloc::boxed::Box<dyn crate::kernel::fd::FileRepr>, crate::kernel::fs::FSError>
    {
        Ok(Box::new(OwnedStdin::new()) as Box<dyn FileRepr>)
    }
}

impl TTYSource for StdInFileFactory {
    fn read(&self) -> Option<u8> {
        let pid = tls::task_data().current_thread()?.pid();
        self.ensure_init(pid);
        self.delegate(&pid, |stdin| stdin.read()).flatten()
    }

    fn read_buf(&self, buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        let pid = tls::task_data()
            .current_thread()
            .ok_or(FSError::simple(crate::kernel::fs::FSErrorKind::NotFound))?
            .pid();
        self.ensure_init(pid);
        self.delegate(&pid, |stdin| stdin.read_buf(buf, offset))
            .ok_or(FSError::simple(crate::kernel::fs::FSErrorKind::NotFound))
            .flatten()
    }
}

impl_empty_write!(StdInFileFactory);
impl_read_for_tty!(StdInFileFactory);
impl_file_for_wr!(StdInFileFactory: NodeType::File);

#[derive(Debug)]
pub struct OwnedStdin {
    cursor: AtomicUsize,
}

impl Clone for OwnedStdin {
    fn clone(&self) -> Self {
        Self {
            cursor: self.cursor.load(Ordering::Relaxed).into(),
        }
    }
}

impl OwnedStdin {
    pub fn new() -> Self {
        Self {
            cursor: KEYBOARD_BUFFER.get_current().into(),
        }
    }
}

impl TTYSource for OwnedStdin {
    fn read(&self) -> Option<u8> {
        let current = self.cursor.load(Ordering::Relaxed);
        if KEYBOARD_BUFFER.is_up_to_date(current) {
            return None;
        }
        if !KEYBOARD_BUFFER.cursor_is_valid(current) {
            self.cursor
                .store(KEYBOARD_BUFFER.get_current(), Ordering::Relaxed);
        }
        let r = KEYBOARD_BUFFER.read1(self.cursor.load(Ordering::Relaxed));
        if r.is_some() {
            self.cursor.fetch_add(1, Ordering::Relaxed);
        }
        r
    }

    fn read_buf(&self, mut buf: &mut [u8], offset: usize) -> crate::kernel::io::IOResult<usize> {
        let cursor = self.cursor.load(Ordering::Relaxed) + offset;
        if KEYBOARD_BUFFER.is_up_to_date(cursor) {
            return Ok(0);
        }
        if !KEYBOARD_BUFFER.cursor_is_valid(cursor) {
            self.cursor.store(
                KEYBOARD_BUFFER
                    .get_current()
                    .saturating_sub(buf.len().min(STDIN_QUEUE_SIZE)),
                Ordering::Relaxed,
            );
        }
        let mut intermediate_buf = alloc::vec![0; buf.len()];
        let n_read =
            KEYBOARD_BUFFER.readn(self.cursor.load(Ordering::Relaxed), &mut intermediate_buf);
        self.cursor.fetch_add(n_read, Ordering::Relaxed);

        let mut n_mapped = 0;
        for &byte in &intermediate_buf[..n_read] {
            if let Ok(res) = parse_scancode(byte) {
                let mapped_bytes = map_key(res, buf);
                if mapped_bytes < 0 {
                    break;
                }
                buf = &mut buf[mapped_bytes as usize..];
                n_mapped += mapped_bytes as usize;
            }
        }
        Ok(n_mapped)
    }
}

impl_empty_write!(OwnedStdin);
impl_read_for_tty!(OwnedStdin);
impl_file_for_wr!(OwnedStdin: NodeType::File);

impl Default for OwnedStdin {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct KeyboardBackend;

impl KeyboardBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl TTYSource for KeyboardBackend {
    fn read(&self) -> Option<u8> {
        KEYBOARD_BUFFER.read1(KEYBOARD_BUFFER.get_current())
    }

    fn read_buf(&self, mut buf: &mut [u8], _offset: usize) -> crate::kernel::io::IOResult<usize> {
        let mut intermediate_buf = alloc::vec![0; buf.len()];

        let mut n_read = 0;
        let mut buf_iter = intermediate_buf.iter_mut();
        while let Some(next_idx) = buf_iter.next()
            && let Some(read) = self.read()
        {
            *next_idx = read;
            n_read += 1;
        }

        let mut n_mapped = 0;
        for &byte in &intermediate_buf[..n_read] {
            if let Ok(res) = parse_scancode(byte) {
                let mapped_bytes = map_key(res, buf);
                if mapped_bytes < 0 {
                    break;
                }
                buf = &mut buf[mapped_bytes as usize..];
                n_mapped += mapped_bytes as usize;
            }
        }
        Ok(n_mapped)
    }
}

impl_read_for_tty!(KeyboardBackend);
impl_empty_write!(KeyboardBackend);
impl_file_for_wr!(KeyboardBackend: NodeType::File);
