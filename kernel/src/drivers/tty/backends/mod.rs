mod framebuffer;
mod keyboard;
mod serial;
mod vga;

pub trait TTYBackend {
    fn write_byte(&self, byte: u8);
    fn flush(&self);
}
