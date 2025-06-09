use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};

use os_macros::kernel_test;

use crate::kernel::threading;

const WRITER_LOCK: usize = usize::MAX;

pub struct RWLock<T> {
    lock: AtomicUsize,
    value: UnsafeCell<T>,
}
unsafe impl<T> Sync for RWLock<T> {}
unsafe impl<T> Send for RWLock<T> {}

#[allow(dead_code)]
impl<T> RWLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            lock: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
        }
    }

    pub fn write(&self) -> WriteGuard<'_, T> {
        while !self
            .lock
            .compare_exchange(0, WRITER_LOCK, Ordering::Acquire, Ordering::Acquire)
            .is_err()
        {
            threading::yield_now();
        }
        WriteGuard { inner: self }
    }

    pub fn try_write(&self) -> Result<WriteGuard<'_, T>, RWLockError> {
        if self
            .lock
            .compare_exchange(0, WRITER_LOCK, Ordering::Acquire, Ordering::Acquire)
            .is_err()
        {
            return Err(RWLockError::IsLocked);
        }
        Ok(WriteGuard { inner: self })
    }

    pub fn read(&self) -> ReadGuard<'_, T> {
        loop {
            if let Ok(guard) = self.try_read() {
                return guard;
            }
            threading::yield_now();
        }
        unreachable!()
    }

    pub fn try_read(&self) -> Result<ReadGuard<'_, T>, RWLockError> {
        let mut val = self.lock.swap(WRITER_LOCK, Ordering::Acquire);
        if val == WRITER_LOCK {
            return Err(RWLockError::IsLocked);
        }
        val += 1;
        self.lock.store(val, Ordering::Release);
        Ok(ReadGuard { inner: self })
    }

    pub fn drop_read(&self) {
        self.lock.fetch_sub(1, Ordering::Release);
    }

    pub fn drop_write(&self) {
        self.lock.store(0, Ordering::Release);
    }
}

pub struct WriteGuard<'a, T> {
    inner: &'a RWLock<T>,
}

impl<T> WriteGuard<'_, T> {}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.inner.value.get()) }
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.inner.value.get()) }
    }
}

impl<T> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        self.inner.drop_write()
    }
}

pub struct ReadGuard<'a, T> {
    inner: &'a RWLock<T>,
}

impl<T> ReadGuard<'_, T> {}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.inner.value.get()) }
    }
}

impl<T> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        self.inner.drop_read()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RWLockError {
    IsLocked,
}

#[cfg(feature = "test_run")]
mod tests {
    use alloc::{sync::Arc, vec::Vec};

    use super::*;

    #[kernel_test]
    fn rwlock_basic() {
        let lock = RWLock::new(0);
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
        let lock = Arc::new(RWLock::new(0));
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
}
