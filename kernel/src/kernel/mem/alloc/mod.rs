use linked_list::SafeHeap;

mod linked_list;

#[global_allocator]
pub static GLOBAL_ALLOCATOR: SafeHeap = linked_list::get_alloc();
