use crossbeam::queue::SegQueue;

use crate::drivers::tty::backends::TTYBackend;

pub struct TTY<'a> {
    backend: &'a TTYBackend,
    input: SegQueue<u8>,
    output: SegQueue<u8>,
}
