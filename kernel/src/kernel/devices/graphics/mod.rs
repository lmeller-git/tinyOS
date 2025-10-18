use alloc::format;

use crate::kernel::{
    fs::{OpenOptions, Path, open},
    graphics::{
        GLOBAL_FRAMEBUFFER,
        framebuffers::{FrameBuffer, get_config},
    },
    io::Write,
};

pub(super) fn init() {
    let basic_config = get_config();
    let fb = &GLOBAL_FRAMEBUFFER;

    let mut gfx_config_file = open(
        Path::new("/ram/.devconf/gfx/config.conf"),
        OpenOptions::CREATE_ALL | OpenOptions::WRITE,
    )
    .unwrap();

    let fmt_str = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        basic_config.red_mask_shift,
        basic_config.red_mask_size,
        basic_config.green_mask_shift,
        basic_config.green_mask_size,
        basic_config.blue_mask_shift,
        basic_config.blue_mask_size,
        fb.bpp(),
        fb.width(),
        fb.height(),
        fb.pitch()
    );
    let bytes = fmt_str.as_bytes();

    gfx_config_file.write_all(bytes, 0).unwrap();
}
