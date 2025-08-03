use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

use crate::{arch::interrupt, kernel::threading::schedule::current_pid};

#[derive(Debug)]
pub struct Gkl {
    lock: AtomicBool,
    count: AtomicUsize,
    currently_held: AtomicU64,
}

impl Gkl {
    pub const fn new() -> Self {
        Self {
            lock: AtomicBool::new(false),
            count: AtomicUsize::new(0),
            currently_held: AtomicU64::new(0),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.lock.load(Ordering::Acquire)
    }

    pub fn lock(&self) -> GklGuard<'_> {
        #[cfg(not(feature = "gkl"))]
        return GklGuard { inner: self };
        loop {
            if let Ok(guard) = self.try_lock() {
                return guard;
            }
            assert!(interrupt::are_enabled());
            spin_loop();
        }
        unreachable!()
    }

    pub fn try_lock(&self) -> Result<GklGuard<'_>, GklErr> {
        #[cfg(not(feature = "gkl"))]
        return Ok(GklGuard { inner: self });
        if self.lock.swap(true, Ordering::Acquire) {
            let pid = self.currently_held.load(Ordering::Acquire);
            if pid == current_pid() {
                self.count.fetch_add(1, Ordering::Release);
                Ok(GklGuard { inner: self })
            } else {
                Err(GklErr::IsLocked)
            }
        } else {
            self.count.fetch_add(1, Ordering::Release);
            self.currently_held.store(current_pid(), Ordering::Release);
            Ok(GklGuard { inner: self })
        }
    }

    pub fn unlock(&self) {
        #[cfg(not(feature = "gkl"))]
        return;
        let count = self.count.fetch_sub(1, Ordering::Release);
        // as the value pre sub is fetch, need to check if it WAS 1
        if count == 1 {
            // store 0 (in theory i do not need to store anything here, as this is only checked if lock is acqiured), but 0 is safe always
            self.currently_held.store(0, Ordering::Release);
            self.lock.store(false, Ordering::Release);
        }
    }

    pub unsafe fn unlock_unchecked(&self) {
        #[cfg(not(feature = "gkl"))]
        return;
        self.count.store(0, Ordering::Release);
        self.currently_held.store(0, Ordering::Release);
        self.lock.store(false, Ordering::Release);
    }
}

impl Default for Gkl {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for Gkl {}
unsafe impl Send for Gkl {}

#[derive(Debug)]
pub struct GklGuard<'a> {
    inner: &'a Gkl,
}

impl Drop for GklGuard<'_> {
    fn drop(&mut self) {
        self.inner.unlock();
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GklErr {
    IsLocked,
}
