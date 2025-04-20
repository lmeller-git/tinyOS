use core::fmt::Write;

use crate::{
    drivers::graphics::{
        GLOBAL_FRAMEBUFFER,
        colors::RGBColor,
        framebuffers::{GlobalFrameBuffer, LimineFrameBuffer},
    },
    serial_println,
    services::graphics::{self, GraphicsError},
};
use conquer_once::spin::OnceCell;
use embedded_graphics::prelude::DrawTarget;
use lazy_static::lazy_static;
use render::BasicTermRender;
use spin::Mutex;

mod logic;
mod parse;
mod render;

// this is the max chars of the current GLOBAL_FRAMBUFFER using the current font in term/render/mod.rs
const MAX_CHARS: usize = 51;

pub type EarlyTermBackend<'a> = Mutex<
    BasicTermRender<'a, graphics::Simplegraphics<'a, GlobalFrameBuffer>, MAX_CHARS, MAX_CHARS>,
>;

pub struct EarlyBootTerm<'a> {
    gfx: &'a EarlyTermBackend<'a>,
}

impl<'a> EarlyBootTerm<'a> {
    fn new(gfx: &'a EarlyTermBackend<'a>) -> Self {
        Self { gfx }
    }
}

impl Write for EarlyBootTerm<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        write!(self.gfx.lock(), "{}", s)
    }
}

// TODO clean up the mess and rewrite graphics shit

static FOO: OnceCell<Mutex<graphics::Simplegraphics<'static, GlobalFrameBuffer>>> =
    OnceCell::uninit();

pub static FOOBAR: OnceCell<
    Mutex<
        BasicTermRender<
            'static,
            graphics::Simplegraphics<'static, GlobalFrameBuffer>,
            MAX_CHARS,
            MAX_CHARS,
        >,
    >,
> = OnceCell::uninit();

pub fn init_term() {
    _ = FOO.try_init_once(|| Mutex::new(graphics::Simplegraphics::new(&GLOBAL_FRAMEBUFFER)));
    _ = FOOBAR.try_init_once(|| Mutex::new(BasicTermRender::new(FOO.try_get().unwrap())));
}

// lazy_static! {
//     pub static ref FOO: Mutex<graphics::Simplegraphics<'static, GlobalFrameBuffer>> =
//         Mutex::new(graphics::Simplegraphics::new(&GLOBAL_FRAMEBUFFER));
// }

// lazy_static! {
//     pub static ref BAR: Mutex<
//         BasicTermRender<
//             'static,
//             graphics::Simplegraphics<'static, GlobalFrameBuffer>,
//             MAX_CHARS,
//             MAX_CHARS,
//         >,
//     > = Mutex::new(BasicTermRender::new(&FOO));
// }

// lazy_static! {
//     pub static ref EARLY_BOOT_TERM: Mutex<EarlyBootTerm<'static>> =
//         Mutex::new(EarlyBootTerm::new(&BAR));
// }

// pub static EARLY_BOOT_TERM: Mutex<Option<EarlyBootTerm<'static>>> = Mutex::new(None);
// pub struct EarlyBootTerm<'a, B>
// where
//     B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
// {
//     gfx: BasicTermRender<'a, B>,
// }

// impl<'a, B> EarlyBootTerm<'a, B>
// where
//     B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
// {
//     fn new(gfx: &'a mut B) -> Self {
//         Self {
//             gfx: BasicTermRender::new(gfx),
//         }
//     }
// }

// impl<B> Write for EarlyBootTerm<'_, B>
// where
//     B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
// {
//     fn write_str(&mut self, s: &str) -> core::fmt::Result {
//         write!(self.gfx, "{}", s)
//     }
// }
