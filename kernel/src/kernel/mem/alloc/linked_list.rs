use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::{NonNull, null_mut},
};

use linked_list_allocator::Heap;

use crate::sync::{
    YieldWaiter,
    locks::{GenericMutex, GenericMutexGuard},
};

pub(super) const fn get_alloc() -> SafeHeap {
    SafeHeap::new()
}

pub struct SafeHeap {
    inner: GenericMutex<Heap, YieldWaiter>,
}

impl SafeHeap {
    pub const fn new() -> Self {
        Self {
            inner: GenericMutex::new(Heap::empty()),
        }
    }

    pub fn init(&self, heap_bottom: *mut u8, heap_size: usize) {
        unsafe {
            self.inner.lock().init(heap_bottom, heap_size);
        }
    }

    pub fn lock(&self) -> GenericMutexGuard<Heap, YieldWaiter> {
        self.inner.lock()
    }
}

unsafe impl GlobalAlloc for SafeHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.lock().allocate_first_fit(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Some(nn_ptr) = NonNull::new(ptr) {
            unsafe { self.lock().deallocate(nn_ptr, layout) };
        }
    }
}
