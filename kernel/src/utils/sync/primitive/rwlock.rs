use core::usize;

use lock_api::{
    GuardSend, RawRwLock, RawRwLockDowngrade, RawRwLockUpgrade, RawRwLockUpgradeDowngrade,
};

use crate::sync::{
    WaitStrategy,
    primitive::semaphore::{RawSemaphore, StaticSemaphore},
};

unsafe impl<S: WaitStrategy> RawRwLock for StaticSemaphore<{ usize::MAX }, S> {
    const INIT: Self = Self::new();
    type GuardMarker = GuardSend;

    fn lock_shared(&self) {
        self.down();
    }

    fn try_lock_shared(&self) -> bool {
        self.try_down().is_ok()
    }

    unsafe fn unlock_shared(&self) {
        unsafe { self.up() };
    }

    fn lock_exclusive(&self) {
        self.down_n(usize::MAX);
    }

    fn try_lock_exclusive(&self) -> bool {
        self.try_down_n(usize::MAX).is_ok()
    }

    unsafe fn unlock_exclusive(&self) {
        unsafe {
            self.up_n(usize::MAX);
        }
    }
}

unsafe impl<S: WaitStrategy> RawRwLockDowngrade for StaticSemaphore<{ usize::MAX }, S> {
    unsafe fn downgrade(&self) {
        unsafe {
            self.up_n(usize::MAX - 1);
        }
    }
}
