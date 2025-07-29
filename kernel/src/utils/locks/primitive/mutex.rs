use core::{
    cell::UnsafeCell,
    fmt::Debug,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};
use os_macros::kernel_test;

use crate::locks::{GKL, GklGuard};

pub struct Mutex<T> {
    lock: AtomicBool,
    value: UnsafeCell<T>,
}
unsafe impl<T> Sync for Mutex<T> {}
unsafe impl<T> Send for Mutex<T> {}

#[allow(dead_code)]
impl<T> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        #[cfg(feature = "gkl")]
        let gkl = GKL.lock();
        while self.lock.swap(true, Ordering::Acquire) {
            spin_loop()
        }
        MutexGuard {
            inner: self,
            #[cfg(feature = "gkl")]
            gkl,
        }
    }
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, MutexError> {
        #[cfg(feature = "gkl")]
        let Ok(gkl) = GKL.try_lock() else {
            return Err(MutexError::IsLocked);
        };
        if self.lock.swap(true, Ordering::Acquire) {
            Err(MutexError::IsLocked)
        } else {
            Ok(MutexGuard {
                inner: self,
                #[cfg(feature = "gkl")]
                gkl,
            })
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

    pub unsafe fn force_unlock(&self) {
        self.unlock();
    }

    pub unsafe fn force_lock(&self) {
        self.lock.store(true, Ordering::Release);
    }

    pub fn is_locked(&self) -> bool {
        self.lock.load(Ordering::Acquire)
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn inner_unchecked(&self) -> &mut T {
        unsafe { self.value.as_mut_unchecked() }
    }
}

#[allow(dead_code)]
pub struct MutexGuard<'a, T> {
    inner: &'a Mutex<T>,
    #[cfg(feature = "gkl")]
    gkl: GklGuard<'a>,
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

impl<T: Debug> Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "MutexGuard {:#?}", unsafe {
            self.inner.value.get().as_ref()
        })
    }
}

impl<T: Debug> Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "MutexGuard {:#?}", self.try_lock())
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
