use core::{
    cell::UnsafeCell,
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use crossbeam::queue::{ArrayQueue, SegQueue};
use os_macros::kernel_test;

use crate::{
    kernel::threading::{
        self,
        schedule::{self, GlobalTaskPtr, current_task},
        task::TaskRepr,
    },
    locks::{GKL, GklGuard},
};

pub struct Mutex<T> {
    lock: AtomicBool,
    value: UnsafeCell<T>,
    // waker_queue: ArrayQueue<GlobalTaskPtr>,
}
unsafe impl<T> Sync for Mutex<T> {}
unsafe impl<T> Send for Mutex<T> {}

#[allow(dead_code)]
impl<T> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        #[cfg(feature = "gkl")]
        let gkl = loop {
            if let Ok(gkl) = GKL.try_lock() {
                break gkl;
            } else {
                threading::yield_now();
            }
        };
        while self.lock.swap(true, Ordering::Acquire) {
            threading::yield_now();
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
        self.lock.store(false, Ordering::Release);
    }

    pub const fn new(value: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            value: UnsafeCell::new(value),
            // waker_queue: ArrayQueue::new(10),
        }
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub unsafe fn force_unlock(&self) {
        self.lock.store(false, Ordering::Release);
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

impl<T> From<T> for Mutex<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[allow(dead_code)]
pub struct MutexGuard<'a, T> {
    inner: &'a Mutex<T>,
    #[cfg(feature = "gkl")]
    gkl: GklGuard<'a>,
}

unsafe impl<T: Send> Send for MutexGuard<'_, T> {}
unsafe impl<T: Send + Sync> Sync for MutexGuard<'_, T> {}

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

impl<T: Debug> Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "MutexGuard {:#?}", unsafe {
            self.inner.value.get().as_ref()
        })
    }
}

impl<T: Debug> Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Mutex {:#?}, locked: {:#?}", self.try_lock(), self.lock)?;
        Ok(())
    }
}

impl<T: PartialEq> PartialEq for Mutex<T> {
    fn eq(&self, other: &Self) -> bool {
        let lock = self.lock();
        let other_lock = other.try_lock(); // assuming other == self. if not and this fails, this migtj lead to problems
        self.value.get() == other.value.get()
    }
}

impl<T: PartialEq + Eq> Eq for Mutex<T> {}

#[derive(Debug, PartialEq, Eq)]
pub enum MutexError {
    IsLocked,
}

#[cfg(feature = "test_run")]
mod tests {
    use alloc::{sync::Arc, vec::Vec};

    use super::*;

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

    #[kernel_test]
    fn concurrent() {
        let lock = Arc::new(Mutex::new(0));
        let mut handles = Vec::new();
        for _ in 0..5 {
            let lock = lock.clone();
            handles.push(
                threading::spawn(move || {
                    for _ in 0..1000 {
                        *lock.lock() += 1;
                        threading::yield_now();
                    }
                })
                .unwrap(),
            );
        }
        for handle in &handles {
            assert!(handle.wait().is_ok());
        }
        assert_eq!(*lock.lock(), 5000);
    }
}
