use core::marker::PhantomData;

use thiserror::Error;

use crate::{
    arch,
    kernel::threading::{self, task::TaskID},
};

mod primitive;

pub mod locks {
    use crate::sync::{
        YieldWaiter,
        primitive::{rwlock::SemaRwLock, semaphore::StaticSemaphore},
    };

    pub type Mutex<T> = lock_api::Mutex<StaticSemaphore<1, YieldWaiter>, T>;
    pub type RwLock<T> = lock_api::RwLock<SemaRwLock<YieldWaiter>, T>;
}

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum SyncErr {
    #[error("tried to access a contended lock")]
    LockContended,
    #[cfg(feature = "gkl")]
    #[error("GKL is not free")]
    GKLHeld,
}

pub(crate) trait StatelessWaitStrategy {
    fn wait();
}

pub(crate) trait WaitStrategy {
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

struct SpinWaiter;

impl StatelessWaitStrategy for SpinWaiter {
    #[inline]
    fn wait() {
        arch::hlt();
    }
}

struct YieldWaiter;

impl StatelessWaitStrategy for YieldWaiter {
    #[inline]
    fn wait() {
        threading::yield_now();
    }
}
