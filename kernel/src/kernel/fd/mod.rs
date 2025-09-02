use crate::kernel::io::{Read, Write};

pub type FileDescriptor = u32;

pub const STDIN_FILENO: FileDescriptor = 0;
pub const STDOUT_FILENO: FileDescriptor = 1;
pub const STDERR_FILENO: FileDescriptor = 2;

pub trait IOCapable: Read + Write {}
