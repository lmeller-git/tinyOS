mod alloc;
mod heap;
mod paging;

pub fn init() {
    paging::init();
    heap::init();
}
