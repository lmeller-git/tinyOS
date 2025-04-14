use super::colors::RGBColor;

pub trait FrameBuffer {
    fn set_pixel(&self, value: &RGBColor, x: usize, y: usize);
    fn clear_pixel(&self, x: usize, y: usize);
    fn clear_all(&self);
    fn fill(&self, value: RGBColor); // TODO add some Area struct?
    fn flush(&self);
    fn width(&self) -> usize;
    fn height(&self) -> usize;
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
}
