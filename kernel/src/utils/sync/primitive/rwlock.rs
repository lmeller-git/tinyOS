use core::usize;

use lock_api::{GuardSend, RawRwLock};

use crate::sync::{
    WaitStrategy,
    primitive::semaphore::{RawSemaphore, StaticSemaphore},
};

pub(crate) struct SemaRwLock<S: WaitStrategy> {
    sema: StaticSemaphore<{ usize::MAX }, S>,
}

impl<S: WaitStrategy> SemaRwLock<S> {
    pub const fn new() -> Self {
        Self {
            sema: StaticSemaphore::new(),
        }
    }
}

unsafe impl<S: WaitStrategy> RawRwLock for SemaRwLock<S> {
    const INIT: Self = Self::new();
    type GuardMarker = GuardSend;

    fn lock_shared(&self) {
        self.sema.down();
    }

    fn try_lock_shared(&self) -> bool {
        self.sema.try_down().is_ok()
    }

    unsafe fn unlock_shared(&self) {
        unsafe { self.sema.up() };
    }

    fn lock_exclusive(&self) {
        self.sema.down_n(usize::MAX);
    }

    fn try_lock_exclusive(&self) -> bool {
        self.sema.try_down_n(usize::MAX).is_ok()
    }

    unsafe fn unlock_exclusive(&self) {
        unsafe {
            self.sema.up_n(usize::MAX);
        }
    }
}
