use core::{
    cell::UnsafeCell,
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};

use os_macros::kernel_test;

use crate::kernel::threading;

const WRITER_LOCK: usize = usize::MAX;

#[derive(Default)]
pub struct RwLock<T> {
    lock: AtomicUsize,
    value: UnsafeCell<T>,
}
unsafe impl<T> Sync for RwLock<T> {}
unsafe impl<T> Send for RwLock<T> {}

#[allow(dead_code)]
impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            lock: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
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
        self.lock
            .compare_exchange(0, WRITER_LOCK, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| RwLockError::IsLocked)
            .map(|_| RwLockWriteGuard { inner: self })
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
        self.lock
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |lock| {
                lock.checked_add(1)
            })
            .map_err(|_| RwLockError::IsLocked)
            .map(|_| RwLockReadGuard { inner: self })
    }

    pub fn drop_read(&self) {
        self.lock.fetch_sub(1, Ordering::Release);
    }

    pub fn drop_write(&self) {
        self.lock.store(0, Ordering::Release);
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

impl<T> From<T> for RwLock<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

pub struct RwLockWriteGuard<'a, T> {
    inner: &'a RwLock<T>,
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
        self.inner.drop_read()
    }
}

impl<T> Debug for RwLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RwLock, lock count: {:#?}", self.lock)?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RwLockError {
    IsLocked,
}

#[cfg(feature = "test_run")]
mod tests {
    use alloc::{sync::Arc, vec::Vec};

    use super::*;

    #[kernel_test]
    fn rwlock_basic() {
        let lock = RwLock::new(0);
        {
            let reader1 = lock.try_read().unwrap();
            assert_eq!(*reader1, 0);
            assert!(lock.try_write().is_err());
        }
        let mut writer1 = lock.try_write().unwrap();
        *writer1 = 42;
        assert!(lock.try_read().is_err());
        drop(writer1);
        let reader1 = lock.try_read().unwrap();
        let reader2 = lock.try_read().unwrap();
        assert_eq!(*reader1, *reader2);
        assert_eq!(*reader1, 42);
    }

    #[kernel_test]
    fn rwlock_concurrent() {
        let lock = Arc::new(RwLock::new(0));
        let mut handles = Vec::new();

        for _ in 0..5 {
            let lock = lock.clone();
            handles.push(
                threading::spawn(move || {
                    for _ in 0..1000 {
                        *lock.write() += 1;
                        threading::yield_now();
                    }
                })
                .unwrap(),
            );
        }
        for handle in &handles {
            assert!(handle.wait().is_ok());
        }
        assert_eq!(*lock.read(), 5000);

        let lock1 = lock.clone();
        let handle1 = threading::spawn(move || {
            let reader = lock1.read();
            for _ in 0..100 {
                assert_eq!(*reader, 5000);
                threading::yield_now();
            }
        })
        .unwrap();

        let handle2 = threading::spawn(move || {
            let reader = lock.read();
            for _ in 0..100 {
                assert_eq!(*reader, 5000);
                threading::yield_now();
            }
        })
        .unwrap();

        assert!(handle1.wait().is_ok());
        assert!(handle2.wait().is_ok());
    }

    #[kernel_test]
    pub fn stress_rwlock() {
        const N_THREADS: usize = 10;
        const N_ITERS: usize = 2_000;

        let lock = Arc::new(RwLock::new(0));
        let mut handles = Vec::new();

        for _ in 0..(N_THREADS / 2) {
            let lock = Arc::clone(&lock);
            handles.push(
                threading::spawn(move || {
                    for _ in 0..N_ITERS {
                        let mut guard = lock.write();
                        *guard += 1;
                        threading::yield_now();
                    }
                })
                .unwrap(),
            );
        }

        for _ in 0..(N_THREADS / 2) {
            let lock = Arc::clone(&lock);
            handles.push(
                threading::spawn(move || {
                    for _ in 0..N_ITERS {
                        let guard = lock.read();
                        let _val = *guard;
                        threading::yield_now();
                    }
                })
                .unwrap(),
            );
        }

        for h in handles {
            h.wait().expect("thread panic");
        }
        let expected = (N_THREADS / 2) * N_ITERS;
        let actual = *lock.read();
        assert_eq!(actual, expected, "expected {}, got {}", expected, actual);
    }
}
