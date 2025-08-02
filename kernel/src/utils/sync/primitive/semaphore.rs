use core::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

use lock_api::GuardSend;

#[cfg(feature = "gkl")]
use crate::locks::GKL;
use crate::sync::{SyncErr, WaitStrategy};

pub unsafe trait RawSemaphore {
    type GuardMaker;
    fn try_down(&self) -> Result<(), SyncErr>;
    fn down(&self);
    fn try_down_n(&self, n: usize) -> Result<(), SyncErr>;
    fn down_n(&self, n: usize);
    unsafe fn up(&self);
    unsafe fn up_n(&self, n: usize);
}

// simple strategies (like spin, yield, ...) are zero sized and will thus be zero-cost

pub struct DynamicSemaphore<S: WaitStrategy> {
    counter: AtomicUsize,
    strategy: S,
}

impl<S: WaitStrategy> DynamicSemaphore<S> {
    pub const fn new(counter: usize) -> Self {
        Self {
            counter: AtomicUsize::new(counter),
            strategy: S::INIT,
        }
    }
}

unsafe impl<S: WaitStrategy> RawSemaphore for DynamicSemaphore<S> {
    type GuardMaker = GuardSend;

    fn try_down(&self) -> Result<(), SyncErr> {
        self.counter
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |counter| {
                counter.checked_sub(1)
            })
            .map_err(|_| SyncErr::LockContended)?;

        #[cfg(feature = "gkl")]
        #[allow(unused_unsafe)]
        {
            let guard = GKL.try_lock().map_err(|_| SyncErr::GKLHeld)?;
            /// # SAFETY This MUST be followed by a call to GKL.unlock() at some point, which is guaranteed under safe usage of the Semaphore. Anything else is ub.
            unsafe {
                core::mem::forget(guard);
            }
        }
        Ok(())
    }

    fn down(&self) {
        loop {
            if self.try_down().is_ok() {
                return;
            }
            self.strategy.wait();
        }
    }

    unsafe fn up(&self) {
        self.counter.fetch_add(1, Ordering::Release);
        #[cfg(feature = "gkl")]
        #[allow(unused_unsafe)]
        /// # SAFETY This MUST be called only after Self::try_down to keep GKL state defined
        unsafe {
            GKL.unlock()
        };
        self.strategy.signal();
    }

    fn try_down_n(&self, n: usize) -> Result<(), SyncErr> {
        self.counter
            .fetch_update(Ordering::Release, Ordering::Relaxed, |counter| {
                counter.checked_sub(n)
            })
            .map_err(|_| SyncErr::LockContended)?;

        #[cfg(feature = "gkl")]
        #[allow(unused_unsafe)]
        {
            let guard = GKL.try_lock().map_err(|_| SyncErr::GKLHeld)?;
            /// # SAFETY This MUST be followed by a call to GKL.unlock() at some point, which is guaranteed under safe usage of the Semaphore. Anything else is ub.
            unsafe {
                core::mem::forget(guard);
            }
        }
        Ok(())
    }

    fn down_n(&self, n: usize) {
        loop {
            if self.try_down_n(n).is_ok() {
                return;
            }
            self.strategy.wait();
        }
    }

    unsafe fn up_n(&self, n: usize) {
        self.counter.fetch_add(n, Ordering::Release);
        #[cfg(feature = "gkl")]
        #[allow(unused_unsafe)]
        /// # SAFETY This MUST be called only after Self::try_down to keep GKL state defined
        unsafe {
            GKL.unlock()
        };
        self.strategy.signal();
    }
}

pub struct StaticSemaphore<const N: usize, S: WaitStrategy> {
    inner: DynamicSemaphore<S>,
}

impl<const N: usize, S: WaitStrategy> StaticSemaphore<N, S> {
    pub const fn new() -> Self {
        Self {
            inner: DynamicSemaphore::new(N),
        }
    }
}

unsafe impl<const N: usize, S: WaitStrategy> RawSemaphore for StaticSemaphore<N, S> {
    type GuardMaker = GuardSend;

    fn try_down(&self) -> Result<(), SyncErr> {
        self.inner.try_down()
    }

    fn down(&self) {
        self.inner.down();
    }

    unsafe fn up(&self) {
        unsafe { self.inner.up() };
    }

    fn try_down_n(&self, n: usize) -> Result<(), SyncErr> {
        self.inner.try_down_n(n)
    }

    fn down_n(&self, n: usize) {
        self.inner.down_n(n);
    }

    unsafe fn up_n(&self, n: usize) {
        unsafe {
            self.inner.up_n(n);
        }
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use alloc::vec;
    use alloc::{sync::Arc, vec::Vec};
    use os_macros::kernel_test;

    use crate::kernel::threading;
    use crate::serial_println;
    use crate::sync::{SpinWaiter, YieldWaiter};

    use super::*;

    #[kernel_test(verbose)]
    fn sema_basic() {
        let sema: StaticSemaphore<2, SpinWaiter> = StaticSemaphore::new();

        assert_eq!(sema.inner.counter.load(Ordering::Relaxed), 2);

        assert!(sema.try_down().is_ok());
        assert!(sema.try_down().is_ok());
        assert!(sema.try_down().is_err());
        assert_eq!(sema.inner.counter.load(Ordering::Relaxed), 0);

        unsafe { sema.up() };
        assert!(sema.try_down().is_ok());

        unsafe { sema.up_n(5) };
        unsafe { sema.up_n(0) }

        assert_eq!(sema.inner.counter.load(Ordering::Relaxed), 5);
    }

    #[kernel_test(verbose)]
    fn sema_concurrent_pc() {
        let sema: Arc<StaticSemaphore<0, YieldWaiter>> = Arc::new(StaticSemaphore::new());

        let mut prod = Vec::new();
        for _ in 0..3 {
            let sema = sema.clone();
            prod.push(
                threading::spawn(move || {
                    for i in 0..5 {
                        #[cfg(feature = "gkl")]
                        // to safely unlock gkl later
                        sema.down_n(0);
                        unsafe { sema.up() };
                        threading::yield_now();
                    }
                })
                .unwrap(),
            );
        }

        let mut consumer = Vec::new();
        for _ in 0..3 {
            let sema = sema.clone();
            consumer.push(
                threading::spawn(move || {
                    let mut items = vec![];
                    for _ in 0..5 {
                        sema.down();
                        items.push(1);
                        #[cfg(feature = "gkl")]
                        // to unlock gkl
                        unsafe {
                            sema.up_n(0)
                        }
                    }
                    items
                })
                .unwrap(),
            );
        }

        for p in prod {
            assert!(p.wait().is_ok());
        }

        let items: usize = consumer
            .into_iter()
            .map(|c| c.wait().unwrap().iter().sum::<usize>())
            .sum();
        assert_eq!(items, 5 * 3);
        assert_eq!(sema.inner.counter.load(Ordering::Relaxed), 0);
    }

    #[kernel_test]
    fn sema_concurrent_simple() {
        let sema: Arc<StaticSemaphore<0, YieldWaiter>> = Arc::new(StaticSemaphore::new());

        let t1 = {
            let sema = sema.clone();
            threading::spawn(move || {
                let mut ret = 0;
                for _ in 0..10 {
                    sema.down();
                    ret += 1;
                    #[cfg(feature = "gkl")]
                    // to unlock gkl
                    unsafe {
                        sema.up_n(0)
                    }
                }
                ret
            })
            .unwrap()
        };

        let t2 = threading::spawn(move || {
            for _ in 0..10 {
                #[cfg(feature = "gkl")]
                // to safely unlock gkl later
                sema.down_n(0);
                unsafe { sema.up() };
                threading::yield_now();
            }
        })
        .unwrap();

        assert!(t2.wait().is_ok());

        assert_eq!(t1.wait().unwrap(), 10);
    }
}
