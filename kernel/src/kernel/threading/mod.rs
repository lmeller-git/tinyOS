pub mod context;
pub mod schedule;
pub mod task;
pub mod trampoline;

pub fn init() {
    schedule::init();
}

#[derive(Debug)]
pub enum ThreadingError {
    StackNotBuilt,
    StackNotFreed,
    PageDirNotBuilt,
}
