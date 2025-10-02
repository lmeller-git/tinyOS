use core::usize;

use lock_api::{GuardSend, RawRwLock, RawRwLockDowngrade};

use crate::sync::{
    WaitStrategy,
    primitive::semaphore::{RawSemaphore, StaticSemaphore},
};

unsafe impl<S: WaitStrategy> RawRwLock for StaticSemaphore<{ usize::MAX }, S> {
    type GuardMarker = GuardSend;

    const INIT: Self = Self::new();

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

#[cfg(feature = "test_run")]
mod tests {
    use os_macros::kernel_test;

    use super::*;
    use crate::sync::SpinWaiter;

    #[kernel_test]
    fn basic_rwlock() {
        let r: StaticSemaphore<{ usize::MAX }, SpinWaiter> = StaticSemaphore::new();

        assert!(r.try_lock_shared());
        assert!(r.is_locked());
        assert!(!r.try_lock_exclusive());
        assert!(r.try_lock_shared());

        unsafe { r.unlock_shared() };
        unsafe { r.unlock_shared() };

        assert!(r.try_lock_exclusive());
        assert!(r.is_locked());
        assert!(!r.try_lock_shared());

        unsafe { r.unlock_exclusive() };

        assert!(!r.is_locked());

        assert!(r.try_lock_exclusive());
        unsafe { r.downgrade() };
        assert!(r.try_lock_shared());
        unsafe { r.unlock_shared() };
        unsafe { r.unlock_shared() };
    }
}
