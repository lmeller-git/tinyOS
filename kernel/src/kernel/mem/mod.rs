use os_macros::kernel_test;

pub mod addr;
pub mod alloc;
pub mod heap;
pub mod paging;

pub fn init_paging() {}

pub fn init() {
    heap::init();
}

pub fn align_up(n: usize, alignment: usize) -> usize {
    (n + alignment - 1) & !(alignment - 1)
}

#[kernel_test]
fn align() {
    let x = 2;
    assert_eq!(align_up(x, 8), 8);
    let x = 25;
    assert_eq!(align_up(x, 16), 32);
    assert_eq!(align_up(x, 64), 64);
}
