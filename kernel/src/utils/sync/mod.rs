use crossbeam::queue::SegQueue;
use thiserror::Error;

use crate::{
    arch::{self},
    kernel::threading::{self, task::TaskID, tls},
};

mod primitive;

pub mod locks {
    use alloc::format;
    use core::fmt::Debug;

    use crate::sync::{
        ReentrancyChecker,
        WaitStrategy,
        YieldWaiter,
        primitive::semaphore::StaticSemaphore,
    };

    pub type GenericMutex<T, S: WaitStrategy> = lock_api::Mutex<StaticSemaphore<1, S>, T>;
    pub type GenericMutexGuard<'a, T, S: WaitStrategy> =
        lock_api::MutexGuard<'a, StaticSemaphore<1, S>, T>;
    pub type GenericRwLock<T, S: WaitStrategy> =
        lock_api::RwLock<StaticSemaphore<{ usize::MAX }, S>, T>;
    pub type GenericRwLockReadGuard<'a, T, S: WaitStrategy> =
        lock_api::RwLockReadGuard<'a, StaticSemaphore<{ usize::MAX }, S>, T>;
    pub type GenericRwLockWriteGuard<'a, T, S: WaitStrategy> =
        lock_api::RwLockWriteGuard<'a, StaticSemaphore<{ usize::MAX }, S>, T>;

    pub type Mutex<T> = GenericMutex<T, YieldWaiter>;
    pub type MutexGuard<'a, T> = GenericMutexGuard<'a, T, YieldWaiter>;
    pub type RwLock<T> = GenericRwLock<T, YieldWaiter>;
    pub type RwLockReadGuard<'a, T> = GenericRwLockReadGuard<'a, T, YieldWaiter>;
    pub type RwLockWriteGuard<'a, T> = GenericRwLockWriteGuard<'a, T, YieldWaiter>;
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

pub struct BlockingWaiter {
    queue: SegQueue<TaskID>,
}

impl WaitStrategy for BlockingWaiter {
    const INIT: Self = Self {
        queue: SegQueue::new(),
    };

    fn wait(&self) {
        self.queue.push(tls::task_data().current_pid());
        tls::task_data().block(&tls::task_data().current_pid());
        threading::yield_now();
    }

    fn signal(&self) {
        if let Some(next) = self.queue.pop() {
            tls::task_data().wake(&next);
        }
    }
}

// TODO rewrite this correctly
pub struct ReentrancyChecker<S: WaitStrategy> {
    currently_held_by: SegQueue<TaskID>,
    strategy: S,
}

impl<S: WaitStrategy> WaitStrategy for ReentrancyChecker<S> {
    const INIT: Self = Self {
        currently_held_by: SegQueue::new(),
        strategy: S::INIT,
    };

    fn wait(&self) {
        let mut tot = 0;
        while let Some(id) = self.currently_held_by.pop() {
            if id == tls::task_data().current_pid() {
                panic!("reeantrancy detected in Task {id:?}")
            }
            tot += 1;
            self.currently_held_by.push(id);
            if tot > self.currently_held_by.len() {
                break;
            }
        }
        self.strategy.wait();
        self.currently_held_by.push(tls::task_data().current_pid());
    }

    fn signal(&self) {
        let mut total = 0;
        while let Some(id) = self.currently_held_by.pop() {
            if id == tls::task_data().current_pid() {
                break;
            } else {
                self.currently_held_by.push(id);
                total += 1;
            }
            if total > self.currently_held_by.len() {
                panic!("a thread tried to release a lock it did not hold");
            }
        }
        self.strategy.signal()
    }
}

pub struct NoBlock;

impl StatelessWaitStrategy for NoBlock {
    fn wait() {
        panic!(
            "Task {:?} tried to block on a NoBlock lock. This is a Bug.",
            tls::task_data().current_pid()
        );
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use alloc::{sync::Arc, vec::Vec};

    use lock_api::RwLockWriteGuard;
    use os_macros::kernel_test;

    use super::*;
    use crate::sync::locks::GenericMutex;

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

    #[kernel_test]
    fn blocking() {
        let lock: Arc<GenericMutex<i32, BlockingWaiter>> = Arc::new(GenericMutex::new(0));

        let mut threads = Vec::new();

        for _ in 0..5 {
            threads.push({
                let lock = lock.clone();
                threading::spawn(move || {
                    for _ in 0..10 {
                        *lock.lock() += 10;
                        threading::yield_now();
                    }
                })
                .unwrap()
            });
        }

        for t in threads {
            assert!(t.wait().is_ok());
        }

        assert_eq!(*lock.lock(), 500);
    }
}
