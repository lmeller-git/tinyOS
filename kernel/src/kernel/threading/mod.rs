pub mod context;
pub mod schedule;
pub mod task;

pub fn init() {
    schedule::init();
}

pub enum ThreadingError {
    StackNotBuilt,
    StackNotFreed,
    PageDirNotBuilt,
}
