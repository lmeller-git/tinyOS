use core::sync::atomic::{AtomicUsize, Ordering};

use os_macros::kernel_test;

#[cfg(feature = "gkl")]
use crate::locks::{GKL, GklGuard};
use crate::{kernel::threading, locks::LockErr};

#[derive(Debug, Default)]
pub(crate) struct RawSemaphore {
    count: AtomicUsize,
}

impl RawSemaphore {
    pub const fn new(init: usize) -> Self {
        Self {
            count: AtomicUsize::new(init),
        }
    }

    pub fn wait(&self) -> RawSemaGuard<'_> {
        loop {
            if let Ok(guard) = self.try_wait() {
                return guard;
            }
            threading::yield_now();
        }
    }

    pub fn try_wait(&self) -> Result<RawSemaGuard<'_>, LockErr> {
        #[cfg(feature = "gkl")]
        let gkl = GKL.try_lock().map_err(|_| LockErr::GKLHeld)?;

        self.count
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |lock| {
                lock.checked_sub(1)
            })
            .map_err(|_| LockErr::AlreadyLocked)
            .map(|_| RawSemaGuard {
                inner: self,
                #[cfg(feature = "gkl")]
                gkl,
            })
    }

    pub unsafe fn update_unchecked<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&AtomicUsize) -> R,
    {
        f(&self.count)
    }

    pub fn count(&self) -> usize {
        self.count.load(Ordering::Acquire) // could be Relaxed, as this is likely not used in critical situations
    }

    // manual increase of counter. Prefer using RAII guard
    pub fn signal(&self) {
        self.count.fetch_add(1, Ordering::Release);
    }

    // manual decrease of counter. Prefer using wait()
    pub fn raw_wait(&self) {
        while let Err(_) = self
            .count
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |lock| {
                lock.checked_sub(1)
            })
        {}
    }
}

pub(crate) struct RawSemaGuard<'a> {
    inner: &'a RawSemaphore,
    #[cfg(feature = "gkl")]
    gkl: GklGuard<'a>,
}

impl Drop for RawSemaGuard<'_> {
    fn drop(&mut self) {
        self.inner.signal();
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use crate::kernel::threading::JoinHandle;

    use super::*;

    use alloc::vec::Vec;
    use alloc::{sync::Arc, vec};

    #[kernel_test]
    fn try_wait_success() {
        let sem = RawSemaphore::new(2);

        // First wait should succeed
        let guard1 = sem.try_wait().expect("First try_wait should succeed");
        assert_eq!(sem.count(), 1);

        // Second wait should succeed
        let guard2 = sem.try_wait().expect("Second try_wait should succeed");
        assert_eq!(sem.count(), 0);

        // Cleanup
        drop(guard1);
        drop(guard2);

        assert_eq!(sem.count(), 2);
    }

    #[kernel_test]
    fn try_wait_failure() {
        let sem = RawSemaphore::new(0);

        // Should fail when count is 0
        match sem.try_wait() {
            Err(LockErr::AlreadyLocked) => { /* expected */ }
            _ => panic!("try_wait should fail when count is 0"),
        }

        assert_eq!(sem.count(), 0);
    }

    #[kernel_test]
    fn test_multiple_guards_drop_order() {
        let sem = RawSemaphore::new(3);

        let guard1 = sem.try_wait().unwrap();
        let guard2 = sem.try_wait().unwrap();
        let guard3 = sem.try_wait().unwrap();

        assert_eq!(sem.count(), 0);

        drop(guard2);
        assert_eq!(sem.count(), 1);

        drop(guard1);
        assert_eq!(sem.count(), 2);

        drop(guard3);
        assert_eq!(sem.count(), 3);
    }

    #[kernel_test]
    fn concurrent_try_wait() {
        let sem = Arc::new(RawSemaphore::new(2));
        let mut handles = vec![];

        for i in 0..4 {
            let sem_clone = Arc::clone(&sem);
            let handle: JoinHandle<Option<usize>> = threading::spawn(move || {
                match sem_clone.try_wait() {
                    Ok(_guard) => {
                        // Hold for a bit to ensure concurrency
                        threading::yield_now();
                        Some(i)
                    }
                    Err(_) => None,
                }
            })
            .unwrap();
            handles.push(handle);
        }

        let results: Vec<_> = handles.into_iter().map(|h| h.wait().unwrap()).collect();

        let successful = results.iter().filter(|r| r.is_some()).count();
        assert!(successful >= 2);
        assert_eq!(sem.count(), 2);
    }

    #[kernel_test]
    fn wait_eventually_succeeds() {
        let sem = Arc::new(RawSemaphore::new(0));
        let sem_clone = Arc::clone(&sem);

        // Spawn thread that will wait
        let waiter = threading::spawn(move || {
            // This should block initially but succeed after signal
            let _guard = sem_clone.wait();
            42 // Return value to confirm we got here
        })
        .unwrap();

        // Give waiter time to start waiting
        for _ in 0..2 {
            threading::yield_now();
        }

        sem.signal();

        // Waiter should complete
        let result = waiter.wait().unwrap();
        assert_eq!(result, 42);
        assert_eq!(sem.count(), 1);
    }

    #[kernel_test]
    fn producer_consumer_pattern() {
        let sem = Arc::new(RawSemaphore::new(0));
        let sem_producer = Arc::clone(&sem);
        let sem_consumer = Arc::clone(&sem);

        let producer = threading::spawn(move || {
            for i in 0..5 {
                threading::yield_now();
                sem_producer.signal(); // Produce item
            }
        })
        .unwrap();

        let consumer = threading::spawn(move || {
            let mut consumed = vec![];
            for _ in 0..5 {
                let _permit = sem_consumer.wait(); // Wait for item
                consumed.push("item");
            }
            consumed
        })
        .unwrap();

        producer.wait().unwrap();
        let items = consumer.wait().unwrap();

        assert_eq!(items.len(), 5);
        assert_eq!(sem.count(), 5);
    }

    #[kernel_test]
    fn raw_producer_consumer_pattern() {
        let sem = Arc::new(RawSemaphore::new(0));
        let sem_producer = Arc::clone(&sem);
        let sem_consumer = Arc::clone(&sem);

        let producer = threading::spawn(move || {
            for i in 0..5 {
                threading::yield_now();
                sem_producer.signal(); // Produce item
            }
        })
        .unwrap();

        let consumer = threading::spawn(move || {
            let mut consumed = vec![];
            for _ in 0..5 {
                let _permit = sem_consumer.raw_wait(); // Wait for item
                consumed.push("item");
            }
            consumed
        })
        .unwrap();

        producer.wait().unwrap();
        let items = consumer.wait().unwrap();

        assert_eq!(items.len(), 5);
        assert_eq!(sem.count(), 0);
    }
}
