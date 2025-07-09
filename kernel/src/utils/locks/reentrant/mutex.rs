use core::{
    cell::UnsafeCell,
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

use crossbeam::queue::{ArrayQueue, SegQueue};
use os_macros::kernel_test;

use crate::{
    kernel::threading::{
        self,
        schedule::{
            self, GLOBAL_SCHEDULER, GlobalTaskPtr, OneOneScheduler, current_pid, current_task,
            with_scheduler_unckecked,
        },
        task::TaskRepr,
    },
    locks::{GKL, GklGuard, thread_safe::MutexError},
    serial_println,
};

pub struct Mutex<T> {
    lock: AtomicBool,
    value: UnsafeCell<T>,
    held_by: AtomicU64,
    count: AtomicUsize,
    // waker_queue: ArrayQueue<GlobalTaskPtr>,
}
unsafe impl<T> Sync for Mutex<T> {}
unsafe impl<T> Send for Mutex<T> {}

#[allow(dead_code)]
impl<T> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            if let Ok(guard) = self.try_lock() {
                return guard;
            }
            // if GLOBAL_SCHEDULER.is_initialized() {
            //     unsafe {
            //         with_scheduler_unckecked(|sched| {
            //             if let Some(current) = sched.current_mut().as_mut() {
            //                 if current.raw().try_write().map(|mut t| t.block()).is_ok() {
            //                     self.waker_queue.push(current.clone());
            //                 }
            //             }
            //         })
            //     }
            // }
            threading::yield_now();
        }
    }

    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, MutexError> {
        let Ok(gkl) = GKL.try_lock() else {
            return Err(MutexError::IsLocked);
        };
        if self.lock.swap(true, Ordering::Acquire) {
            if self.held_by.load(Ordering::Acquire) == current_pid() {
                self.count.fetch_add(1, Ordering::Release);
                Ok(MutexGuard { inner: self, gkl })
            } else {
                Err(MutexError::IsLocked)
            }
        } else {
            self.held_by.store(current_pid(), Ordering::Release);
            self.count.fetch_add(1, Ordering::Release);
            Ok(MutexGuard { inner: self, gkl })
        }
    }

    fn unlock(&self) {
        let count = self.count.fetch_sub(1, Ordering::Release);
        if count == 1 {
            self.held_by.store(0, Ordering::Release); // this is not strictly necessary, as the next locker will set this to self
            self.lock.store(false, Ordering::Release);
        }
        // if let Some(task) = self.waker_queue.pop() {
        //     if let Some(mut sched) = schedule::get() {
        //         // gives a potential writer the chance to acquire the lock
        //         task.with_inner_mut(|inner| inner.wake());
        //         sched.wake(&task.read_inner().pid);
        //     }
        // }
    }

    pub const fn new(value: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            value: UnsafeCell::new(value),
            held_by: AtomicU64::new(0),
            count: AtomicUsize::new(0),
            // waker_queue: ArrayQueue::new(10),
        }
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub unsafe fn force_unlock(&self) {
        self.count.fetch_sub(1, Ordering::Release);
        self.lock.store(false, Ordering::Release);
        // if let Some(task) = self.waker_queue.pop() {
        //     if let Some(mut sched) = schedule::get() {
        //         // gives a potential writer the chance to acquire the lock
        //         task.with_inner_mut(|inner| inner.wake());
        //         sched.wake(&task.read_inner().pid);
        //     }
        // }
    }
    pub unsafe fn force_lock(&self) {
        self.count.fetch_add(1, Ordering::Release);
        self.lock.store(true, Ordering::Release);
    }

    pub fn is_locked(&self) -> bool {
        self.lock.load(Ordering::Acquire)
    }
}

impl<T> From<T> for Mutex<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

pub struct MutexGuard<'a, T> {
    inner: &'a Mutex<T>,
    gkl: GklGuard<'a>,
}

unsafe impl<T: Send> Send for MutexGuard<'_, T> {}
unsafe impl<T: Send + Sync> Sync for MutexGuard<'_, T> {}

impl<T> MutexGuard<'_, T> {}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.inner.value.get()) }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.inner.value.get()) }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.inner.unlock()
    }
}

impl<T: Debug> Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "MutexGuard {:#?}", unsafe {
            self.inner.value.get().as_ref()
        })
    }
}

impl<T: Debug> Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Mutex {:#?}, locked: {:#?}, held_by: {:#?}, count: {:#?}",
            self.try_lock(),
            self.lock,
            self.held_by,
            self.count
        )?;
        Ok(())
    }
}

impl<T: PartialEq> PartialEq for Mutex<T> {
    fn eq(&self, other: &Self) -> bool {
        let lock = self.lock();
        let other_lock = other.try_lock(); // assuming other == self. if not and this fails, this migtj lead to problems
        self.value.get() == other.value.get()
    }
}

impl<T: PartialEq + Eq> Eq for Mutex<T> {}

#[kernel_test]
fn reentrancy() {
    let lock = Mutex::new("foo");

    let guard1 = lock.lock();
    assert!(lock.try_lock().is_ok());
    drop(guard1);
    assert!(!lock.is_locked());

    let guard1 = lock.lock();
    let mut guard2 = lock.lock();
    drop(guard1);
    assert!(lock.is_locked());

    let mut guard3 = lock.lock();
    *guard2 = "foobar\n\t";
    *guard3 = guard3.trim_end();
    drop(guard2);
    drop(guard3);
    assert_eq!(lock.into_inner(), "foobar");
}
