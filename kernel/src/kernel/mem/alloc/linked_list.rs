use linked_list_allocator::LockedHeap;

// pub(super) static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub(super) const fn get_alloc() -> LockedHeap {
    LockedHeap::empty()
}
