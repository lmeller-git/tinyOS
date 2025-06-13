use core::fmt::Debug;

mod sink;
mod source;

pub trait TTYSink: Debug {
    fn write(&self, bytes: &[u8]);
    fn flush(&self);
}

pub trait TTYSource: Debug {
    fn read(&self) -> Option<u8>;
}
