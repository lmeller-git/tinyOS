use lock_api::{GuardSend, RawMutex};
use os_macros::kernel_test;

use crate::sync::{
    SpinWaiter,
    WaitStrategy,
    primitive::semaphore::{RawSemaphore, StaticSemaphore},
};

unsafe impl<S: WaitStrategy> RawMutex for StaticSemaphore<1, S> {
    type GuardMarker = GuardSend;

    const INIT: Self = Self::new();

    fn try_lock(&self) -> bool {
        self.try_down().is_ok()
    }

    fn lock(&self) {
        self.down();
    }

    unsafe fn unlock(&self) {
        unsafe { self.up() };
    }
}

#[kernel_test]
fn mutex_basic() {
    let m: StaticSemaphore<1, SpinWaiter> = StaticSemaphore::new();

    assert!(m.try_lock());
    assert!(m.is_locked());
    assert!(!m.try_lock());

    unsafe { m.unlock() };

    assert!(!m.is_locked());
    assert!(m.try_lock());

    unsafe { m.unlock() }
}
