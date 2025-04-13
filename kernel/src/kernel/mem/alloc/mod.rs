use linked_list_allocator::LockedHeap;

mod linked_list;

#[global_allocator]
pub static GLOBAL_ALLOCATOR: LockedHeap = linked_list::get_alloc();
