use core::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

use lock_api::GuardSend;

#[cfg(feature = "gkl")]
use crate::locks::GKL;
use crate::sync::{SyncErr, WaitStrategy};

pub(crate) unsafe trait RawSemaphore {
    type GuardMaker;
    fn try_down(&self) -> Result<(), SyncErr>;
    fn down(&self);
    fn try_down_n(&self, n: usize) -> Result<(), SyncErr>;
    fn down_n(&self, n: usize);
    unsafe fn up(&self);
    unsafe fn up_n(&self, n: usize);
}

// simple strategies (like spin, yield, ...) are zero sized and will thus be zero-cost

pub(crate) struct DynamicSemaphore<S: WaitStrategy> {
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

pub(crate) struct StaticSemaphore<const N: usize, S: WaitStrategy> {
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
