use core::marker::PhantomData;

use thiserror::Error;

use crate::{
    arch::{self, hcf},
    kernel::threading::{self, task::TaskID},
    locks::GKL,
    serial_println,
};

mod primitive;

pub mod locks {
    use crate::sync::{YieldWaiter, primitive::semaphore::StaticSemaphore};

    pub type Mutex<T> = lock_api::Mutex<StaticSemaphore<1, YieldWaiter>, T>;
    pub type MutexGuard<'a, T> = lock_api::MutexGuard<'a, StaticSemaphore<1, YieldWaiter>, T>;
    pub type RwLock<T> = lock_api::RwLock<StaticSemaphore<{ usize::MAX }, YieldWaiter>, T>;
    pub type RwLockReadGuard<'a, T> =
        lock_api::RwLockReadGuard<'a, StaticSemaphore<{ usize::MAX }, YieldWaiter>, T>;
    pub type RwLockWriteGuard<'a, T> =
        lock_api::RwLockWriteGuard<'a, StaticSemaphore<{ usize::MAX }, YieldWaiter>, T>;
}

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum SyncErr {
    #[error("tried to access a contended lock")]
    LockContended,
    #[cfg(feature = "gkl")]
    #[error("GKL is not free")]
    GKLHeld,
}

pub trait StatelessWaitStrategy {
    fn wait();
}

pub trait WaitStrategy {
    const INIT: Self;
    fn wait(&self);
    fn signal(&self) {}
}

impl<S> WaitStrategy for S
where
    S: StatelessWaitStrategy,
{
    /// # SAFETY: StatelessWaitStrategies will/must always be zero sized.
    const INIT: Self = unsafe {
        const { assert!(core::mem::size_of::<S>() == 0) };
        core::mem::zeroed()
    };

    #[inline]
    fn wait(&self) {
        Self::wait();
    }
}

pub struct SpinWaiter;

impl StatelessWaitStrategy for SpinWaiter {
    #[inline]
    fn wait() {
        arch::hlt();
    }
}

pub struct YieldWaiter;

impl StatelessWaitStrategy for YieldWaiter {
    #[inline]
    fn wait() {
        threading::yield_now();
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use alloc::{sync::Arc, vec::Vec};
    use lock_api::RwLockWriteGuard;
    use os_macros::kernel_test;

    use crate::serial_println;

    use super::*;

    #[kernel_test]
    fn mutex_basic() {
        let mutex = locks::Mutex::new(0);

        let mut guard = mutex.try_lock().unwrap();
        assert!(mutex.try_lock().is_none());
        assert!(mutex.is_locked());

        *guard = 42;

        drop(guard);

        assert!(!mutex.is_locked());
        assert_eq!(*mutex.try_lock().unwrap(), 42);
    }

    #[kernel_test(verbose)]
    fn mutex_concurrent() {
        let mutex: Arc<locks::Mutex<i32>> = Arc::new(locks::Mutex::new(0));

        let mut threads = Vec::new();

        for _ in 0..5 {
            threads.push({
                let mutex = mutex.clone();
                threading::spawn(move || {
                    for _ in 0..10 {
                        *mutex.lock() += 10;
                        threading::yield_now();
                    }
                })
                .unwrap()
            });
        }

        for t in &threads {
            assert!(t.wait().is_ok());
        }

        assert_eq!(*mutex.lock(), 500);
    }

    #[kernel_test]
    fn rwlock_basic() {
        let rw = locks::RwLock::new(0);

        let r1 = rw.try_read().unwrap();
        let r2 = rw.try_read().unwrap();
        assert!(rw.is_locked());
        assert!(!rw.is_locked_exclusive());
        assert!(rw.try_write().is_none());
        assert_eq!(*r1, 0);
        drop(r1);
        drop(r2);

        let mut w1 = rw.try_write().unwrap();
        assert!(rw.is_locked_exclusive());
        assert!(rw.try_write().is_none());
        assert!(rw.try_read().is_none());
        *w1 = 42;

        let r1 = RwLockWriteGuard::downgrade(w1);
        assert!(!rw.is_locked_exclusive());
        let r2 = rw.try_read().unwrap();
        assert_eq!(*r1, 42);
    }

    #[kernel_test]
    fn rwlock_concurrent() {
        let rw = Arc::new(locks::RwLock::new(0));
        let mut threads = Vec::new();

        let mut write = rw.write();

        for i in 0..5 {
            threads.push({
                let rw = rw.clone();
                threading::spawn(move || {
                    let guard = rw.read();
                    assert_eq!(*guard, 42);
                })
                .unwrap()
            });
        }

        for _ in 0..4 {
            *write += 10;
            threading::yield_now();
        }
        *write += 2;
        assert_eq!(*write, 42);
        drop(write);

        for t in threads {
            assert!(t.wait().is_ok());
        }
    }
}
