use core::{
    cell::UnsafeCell,
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

use crossbeam::queue::{ArrayQueue, SegQueue};
use os_macros::kernel_test;

use crate::{
    kernel::threading::{
        self,
        schedule::{
            self, GLOBAL_SCHEDULER, GlobalTaskPtr, OneOneScheduler, current_pid, current_task,
            with_current_task, with_scheduler_unckecked,
        },
        task::TaskRepr,
    },
    locks::{GKL, GklGuard, thread_safe::RwLockError},
    serial_println,
};

const WRITER_LOCK: usize = usize::MAX;

pub struct RwLock<T> {
    lock: AtomicUsize,
    value: UnsafeCell<T>,
    held_by: AtomicU64,
    count: AtomicUsize,
    // waker_queue: ArrayQueue<GlobalTaskPtr>,
}
unsafe impl<T> Sync for RwLock<T> {}
unsafe impl<T> Send for RwLock<T> {}

#[allow(dead_code)]
impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            lock: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
            held_by: AtomicU64::new(0),
            count: AtomicUsize::new(0),
            // waker_queue: ArrayQueue::new(10),
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        loop {
            if let Ok(writer) = self.try_write() {
                return writer;
            }
            threading::yield_now();
        }
    }

    pub fn try_write(&self) -> Result<RwLockWriteGuard<'_, T>, RwLockError> {
        let Ok(gkl) = GKL.try_lock() else {
            return Err(RwLockError::IsLocked);
        };
        if let Ok(_) =
            self.lock
                .compare_exchange(0, WRITER_LOCK, Ordering::Acquire, Ordering::Relaxed)
        // .map_err(|_| RwLockError::IsLocked)
        // .map(|_| RwLockWriteGuard { inner: self, gkl })
        {
            self.held_by.store(current_pid(), Ordering::Release);
            self.count.fetch_add(1, Ordering::Release);
            Ok(RwLockWriteGuard { inner: self, gkl })
        } else {
            if self.held_by.load(Ordering::Acquire) == current_pid() {
                self.count.fetch_add(1, Ordering::Release);
                Ok(RwLockWriteGuard { inner: self, gkl })
            } else {
                Err(RwLockError::IsLocked)
            }
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        loop {
            if let Ok(guard) = self.try_read() {
                return guard;
            }
            threading::yield_now();
        }
    }

    pub fn try_read(&self) -> Result<RwLockReadGuard<'_, T>, RwLockError> {
        let Ok(gkl) = GKL.try_lock() else {
            return Err(RwLockError::IsLocked);
        };
        self.lock
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |lock| {
                lock.checked_add(1)
            })
            .map_err(|_| RwLockError::IsLocked)
            .map(|_| RwLockReadGuard { inner: self, gkl })
    }

    pub fn drop_read(&self) {
        self.lock.fetch_sub(1, Ordering::Release);
    }

    pub fn drop_write(&self) {
        let count = self.count.fetch_sub(1, Ordering::Release);
        if count == 1 {
            self.held_by.store(0, Ordering::Release); // not strictly necessary, as the next locker will set this to self (and 0 is of course also a process)
            self.lock.store(0, Ordering::Release);
        }
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    pub fn update<F: Fn(RwLockWriteGuard<'_, T>)>(&self, func: F) {
        let guard = self.write();
        func(guard)
    }
}

impl<T> From<T> for RwLock<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

pub struct RwLockWriteGuard<'a, T> {
    inner: &'a RwLock<T>,
    gkl: GklGuard<'a>,
}

impl<T> RwLockWriteGuard<'_, T> {}

unsafe impl<T: Send> Send for RwLockWriteGuard<'_, T> {}
unsafe impl<T: Send + Sync> Sync for RwLockWriteGuard<'_, T> {}

impl<T> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.inner.value.get()) }
    }
}

impl<T> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.inner.value.get()) }
    }
}

impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.inner.drop_write()
    }
}

pub struct RwLockReadGuard<'a, T> {
    inner: &'a RwLock<T>,
    gkl: GklGuard<'a>,
}

impl<T> RwLockReadGuard<'_, T> {}

impl<T> Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.inner.value.get()) }
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.inner.drop_read();
    }
}

impl<T: Debug> Debug for RwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RwLockReadGuard {:#?}", unsafe {
            &*self.inner.value.get()
        })
    }
}

impl<T: Debug> Debug for RwLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "RwLock {:#?}, lock count: {:#?}, held_by: {:#?}, count: {:#?}",
            self.try_read(),
            self.lock,
            self.held_by,
            self.count
        )?;
        Ok(())
    }
}

impl<T: Default> Default for RwLock<T> {
    fn default() -> Self {
        Self {
            lock: AtomicUsize::new(0),
            value: UnsafeCell::default(),
            held_by: AtomicU64::new(0),
            count: AtomicUsize::new(0),
            // waker_queue: ArrayQueue::new(10),
        }
    }
}

#[kernel_test]
fn reentrancy() {
    let lock = RwLock::new("hello");
    let writer1 = lock.write();
    assert!(lock.try_read().is_err());
    assert!(lock.try_write().is_ok());
    drop(writer1);

    let reader1 = lock.read();
    assert!(lock.try_write().is_err());
    assert!(lock.try_read().is_ok());
    drop(reader1);

    let mut writer1 = lock.write();
    let mut writer2 = lock.write();

    *writer1 = "foobar\n\t";
    *writer2 = writer2.trim();
    drop(writer1);
    drop(writer2);
    assert_eq!(lock.into_inner(), "foobar");
}
