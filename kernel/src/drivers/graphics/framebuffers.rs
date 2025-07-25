use conquer_once::spin::OnceCell;
use embedded_graphics::primitives::Rectangle;

use crate::{
    bootinfo,
    drivers::graphics::GLOBAL_FRAMEBUFFER,
    kernel::mem::{
        align_up,
        paging::{kernel_map_region, user_map_region},
    },
    serial_println, services,
};

use super::colors::RGBColor;

static FB_CONFIG: OnceCell<FramBufferConfig> = OnceCell::uninit();

pub fn get_config<'a>() -> &'a FramBufferConfig {
    FB_CONFIG.get_or_init(|| FramBufferConfig {
        red_mask_shift: GLOBAL_FRAMEBUFFER.inner.red_mask_shift(),
        red_mask_size: GLOBAL_FRAMEBUFFER.inner.red_mask_size(),
        green_mask_shift: GLOBAL_FRAMEBUFFER.inner.green_mask_shift(),
        green_mask_size: GLOBAL_FRAMEBUFFER.inner.green_mask_size(),
        blue_mask_shift: GLOBAL_FRAMEBUFFER.inner.blue_mask_shift(),
        blue_mask_size: GLOBAL_FRAMEBUFFER.inner.blue_mask_size(),
    })
}

pub fn get_rgb_pixel(color: &RGBColor, config: &FramBufferConfig) -> u32 {
    let red = ((color.0 as u32) & ((1 << config.red_mask_size) - 1)) << config.red_mask_shift;
    let green = ((color.1 as u32) & ((1 << config.green_mask_size) - 1)) << config.green_mask_shift;
    let blue = ((color.2 as u32) & ((1 << config.blue_mask_size) - 1)) << config.blue_mask_shift;
    red | green | blue
}

pub struct FramBufferConfig {
    pub red_mask_shift: u8,
    pub red_mask_size: u8,
    pub green_mask_shift: u8,
    pub green_mask_size: u8,
    pub blue_mask_shift: u8,
    pub blue_mask_size: u8,
}

#[repr(C)]
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct BoundingBox {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl From<Rectangle> for BoundingBox {
    fn from(value: Rectangle) -> Self {
        Self {
            x: value.top_left.x as usize,
            y: value.top_left.y as usize,
            width: value.size.width as usize,
            height: value.size.height as usize,
        }
    }
}

pub type PixelSize = u32;

pub trait FrameBuffer {
    fn addr(&self) -> *mut u8;
    fn bpp(&self) -> u16;
    fn pitch(&self) -> usize;

    fn set_pixel(&self, value: &RGBColor, x: usize, y: usize);
    fn clear_pixel(&self, x: usize, y: usize);
    fn clear_all(&self);
    fn fill(&self, value: RGBColor); // TODO add some Area struct?
    // Deprecated. Use devices
    fn flush(&self);

    fn width(&self) -> usize;
    fn height(&self) -> usize;
    // returns the offset in BYTES to self.addr where addr is a ptr to an array of BYTES
    fn pixel_offset(&self, x: usize, y: usize) -> usize;
}

pub trait HasFrameBuffer<B: FrameBuffer> {
    fn get_framebuffer(&self) -> &B;
}

// 32 bit
pub struct LimineFrameBuffer<'a> {
    inner: limine::framebuffer::Framebuffer<'a>,
}

impl<'a> LimineFrameBuffer<'a> {
    pub fn try_new(
        bufs: &mut impl Iterator<Item = limine::framebuffer::Framebuffer<'a>>,
    ) -> Option<Self> {
        bufs.next().map(|f| Self { inner: f })
    }

    // pub fn new_static<'b>(
    //     bufs: &mut impl Iterator<Item = limine::framebuffer::Framebuffer<'b>>,
    // ) -> Option<LimineFrameBuffer<'b>>
    // where
    //     'b: 'static,
    // {
    //     bufs.next().map(|fb| Self { inner: fb })
    // }

    // pub fn new_static() -> LimineFrameBuffer<'static> {
    //     Self {
    //         inner: bootinfo::FIRST_FRAMEBUFFER,
    //     }
    // }

    fn get_rgb_pixel(&self, color: &RGBColor) -> u32 {
        let red = ((color.0 as u32) & ((1 << self.inner.red_mask_size()) - 1))
            << self.inner.red_mask_shift();
        let green = ((color.1 as u32) & ((1 << self.inner.green_mask_size()) - 1))
            << self.inner.green_mask_shift();
        let blue = ((color.2 as u32) & ((1 << self.inner.blue_mask_size()) - 1))
            << self.inner.blue_mask_shift();
        red | green | blue
    }
}

impl FrameBuffer for LimineFrameBuffer<'_> {
    fn set_pixel(&self, value: &RGBColor, x: usize, y: usize) {
        // TODO verify
        // should be correct:
        // pitch is bytes per row(ie pixels), bpp is bits per pixel, since one byte == 8 bit bpp / 8 byte
        let pixel_offset = y * self.inner.pitch() as usize + x * (self.inner.bpp() / 8) as usize;
        unsafe {
            self.inner
                .addr()
                .add(pixel_offset)
                .cast::<u32>()
                .write(self.get_rgb_pixel(value))
        };
    }

    fn clear_pixel(&self, x: usize, y: usize) {
        self.set_pixel(&RGBColor::default(), x, y);
    }

    fn clear_all(&self) {
        for y in 0..self.inner.height() as usize {
            for x in 0..self.inner.width() as usize {
                self.clear_pixel(x, y);
            }
        }
    }
    fn fill(&self, value: RGBColor) {
        for y in 0..self.inner.height() as usize {
            for x in 0..self.inner.width() as usize {
                self.set_pixel(&value, x, y);
            }
        }
    }
    fn flush(&self) {}

    fn width(&self) -> usize {
        self.inner.width() as usize
    }

    fn height(&self) -> usize {
        self.inner.height() as usize
    }

    fn pixel_offset(&self, x: usize, y: usize) -> usize {
        y * self.inner.pitch() as usize + x * (self.inner.bpp() / 8) as usize
    }

    fn addr(&self) -> *mut u8 {
        self.inner.addr()
    }

    fn bpp(&self) -> u16 {
        self.inner.bpp()
    }

    fn pitch(&self) -> usize {
        self.inner.pitch() as usize
    }
}

// 32 bit
pub struct GlobalFrameBuffer {
    inner: &'static limine::framebuffer::Framebuffer<'static>,
}

impl GlobalFrameBuffer {
    pub fn new_static() -> Self {
        Self {
            inner: &bootinfo::FIRST_FRAMEBUFFER,
        }
    }

    fn get_rgb_pixel(&self, color: &RGBColor) -> u32 {
        let red = ((color.0 as u32) & ((1 << self.inner.red_mask_size()) - 1))
            << self.inner.red_mask_shift();
        let green = ((color.1 as u32) & ((1 << self.inner.green_mask_size()) - 1))
            << self.inner.green_mask_shift();
        let blue = ((color.2 as u32) & ((1 << self.inner.blue_mask_size()) - 1))
            << self.inner.blue_mask_shift();
        red | green | blue
    }
}

impl FrameBuffer for GlobalFrameBuffer {
    fn set_pixel(&self, value: &RGBColor, x: usize, y: usize) {
        // TODO verify
        // should be correct:
        // pitch is bytes per row(ie pixels), bpp is bits per pixel, since one byte == 8 bit bpp / 8 byte
        let pixel_offset = y * self.inner.pitch() as usize + x * (self.inner.bpp() / 8) as usize;
        unsafe {
            self.inner
                .addr()
                .add(pixel_offset)
                .cast::<u32>()
                .write(self.get_rgb_pixel(value))
        };
    }

    fn clear_pixel(&self, x: usize, y: usize) {
        self.set_pixel(&RGBColor::default(), x, y);
    }

    fn clear_all(&self) {
        for y in 0..self.inner.height() as usize {
            for x in 0..self.inner.width() as usize {
                self.clear_pixel(x, y);
            }
        }
    }
    fn fill(&self, value: RGBColor) {
        for y in 0..self.inner.height() as usize {
            for x in 0..self.inner.width() as usize {
                self.set_pixel(&value, x, y);
            }
        }
    }
    fn flush(&self) {}

    fn width(&self) -> usize {
        self.inner.width() as usize
    }

    fn height(&self) -> usize {
        self.inner.height() as usize
    }

    fn pixel_offset(&self, x: usize, y: usize) -> usize {
        y * self.inner.pitch() as usize + x * (self.inner.bpp() / 8) as usize
    }

    fn addr(&self) -> *mut u8 {
        self.inner.addr()
    }

    fn bpp(&self) -> u16 {
        self.inner.bpp()
    }

    fn pitch(&self) -> usize {
        self.inner.pitch() as usize
    }
}

#[repr(C)]
pub struct RawFrameBuffer {
    addr: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bpp: u16,
}

impl RawFrameBuffer {
    // SAFETY: will allocate space of at least pitch * height at addr
    // caller needs to ensure a valid address and valid values for width, height and pitch
    pub unsafe fn new_user(
        addr: crate::arch::mem::VirtAddr,
        width: usize,
        height: usize,
        bpp: u16,
    ) -> Self {
        let pitch = align_up(width * (bpp / 8) as usize, 64);
        serial_println!(
            "pitch: {}, width: {}, height: {}, size: {}",
            pitch,
            width,
            height,
            pitch * height
        );
        user_map_region(addr, pitch * height).expect("could not map memory");
        Self {
            addr: addr.as_mut_ptr(),
            width,
            height,
            pitch,
            bpp,
        }
    }

    // SAFETY: will allocate space of at least pitch * height at addr
    // caller needs to ensure a valid address and valid values for width, height and pitch
    pub unsafe fn new_kernel(
        addr: crate::arch::mem::VirtAddr,
        width: usize,
        height: usize,
        bpp: u16,
    ) -> Self {
        let pitch = align_up(width * (bpp / 8) as usize, 64);
        serial_println!(
            "pitch: {}, width: {}, height: {}, size: {}",
            pitch,
            width,
            height,
            pitch * height
        );
        kernel_map_region(addr, pitch * height).expect("could not map memory");
        Self {
            addr: addr.as_mut_ptr(),
            width,
            height,
            pitch,
            bpp,
        }
    }
}

impl FrameBuffer for RawFrameBuffer {
    fn set_pixel(&self, value: &RGBColor, x: usize, y: usize) {
        let pixel_offset = y * self.pitch() as usize + x * (self.bpp() / 8) as usize;
        unsafe {
            self.addr()
                .add(pixel_offset)
                .cast::<u32>()
                .write(get_rgb_pixel(value, get_config()))
        };
    }

    fn clear_pixel(&self, x: usize, y: usize) {
        self.set_pixel(&RGBColor::default(), x, y);
    }

    fn clear_all(&self) {
        for y in 0..self.height() as usize {
            for x in 0..self.width() as usize {
                self.clear_pixel(x, y);
            }
        }
    }

    fn fill(&self, value: RGBColor) {
        for y in 0..self.height() as usize {
            for x in 0..self.width() as usize {
                self.set_pixel(&value, x, y);
            }
        }
    }

    fn flush(&self) {}

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn pixel_offset(&self, x: usize, y: usize) -> usize {
        y * self.pitch() as usize + x * (self.bpp() / 8) as usize
    }

    fn addr(&self) -> *mut u8 {
        self.addr
    }

    fn bpp(&self) -> u16 {
        self.bpp
    }

    fn pitch(&self) -> usize {
        self.pitch
    }
}

impl Drop for RawFrameBuffer {
    fn drop(&mut self) {
        //TODO: unmap and deallocate Pages of the buffer (currently cannot deallocate anyways)
    }
}
