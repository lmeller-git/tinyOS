pub mod addr;
pub mod alloc;
pub mod heap;
pub mod paging;

pub fn init_paging() {}

pub fn init() {
    heap::init();
}
