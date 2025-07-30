use lock_api::{GuardSend, RawMutex};

use crate::sync::{
    WaitStrategy,
    primitive::semaphore::{RawSemaphore, StaticSemaphore},
};

unsafe impl<S: WaitStrategy> RawMutex for StaticSemaphore<1, S> {
    const INIT: Self = Self::new();
    type GuardMarker = GuardSend;

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
