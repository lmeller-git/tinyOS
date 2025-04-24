pub mod addr;
pub mod alloc;
pub mod heap;
pub mod paging;

pub fn init_paging() {
    paging::init();
}

pub fn init() {
    heap::init();
}
