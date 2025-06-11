use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};
use os_macros::kernel_test;

pub struct Mutex<T> {
    lock: AtomicBool,
    value: UnsafeCell<T>,
}
unsafe impl<T> Sync for Mutex<T> {}
unsafe impl<T> Send for Mutex<T> {}

#[allow(dead_code)]
impl<T> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        while self.lock.swap(true, Ordering::Acquire) {
            spin_loop()
        }
        MutexGuard { inner: self }
    }
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, MutexError> {
        if self.lock.swap(true, Ordering::Acquire) {
            Err(MutexError::IsLocked)
        } else {
            Ok(MutexGuard { inner: self })
        }
    }
    fn unlock(&self) {
        self.lock.store(false, Ordering::Release)
    }
    pub fn new(value: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }
}

pub struct MutexGuard<'a, T> {
    inner: &'a Mutex<T>,
}

impl<T> MutexGuard<'_, T> {}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.inner.value.get()) }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.inner.value.get()) }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.inner.unlock()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MutexError {
    IsLocked,
}

#[kernel_test]
fn mutex() {
    let m = Mutex::new(0);
    assert_eq!(*m.lock(), 0);
    *m.lock() = 42;
    assert_eq!(*m.lock(), 42);
    let lock = m.lock();
    assert!(m.try_lock().is_err());
    drop(lock);
    assert!(m.try_lock().is_ok());
}
