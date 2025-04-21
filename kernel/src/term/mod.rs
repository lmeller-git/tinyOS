#![allow(dead_code)]

use crate::{
    drivers::graphics::{GLOBAL_FRAMEBUFFER, framebuffers::GlobalFrameBuffer},
    services::graphics,
};
use conquer_once::spin::OnceCell;
use core::fmt::{Arguments, Write};
use os_macros::tests;
use render::BasicTermRender;
use spin::Mutex;

mod logic;
mod parse;
mod render;

// this is the max chars of the current GLOBAL_FRAMBUFFER using the current font in term/render/mod.rs
const MAX_CHARS_X: usize = 102;
const MAX_CHARS_Y: usize = 38;

// TODO clean up the mess and rewrite graphics shit

static FOO: OnceCell<Mutex<graphics::Simplegraphics<'static, GlobalFrameBuffer>>> =
    OnceCell::uninit();

static mut BAR: render::TermCharBuffer<MAX_CHARS_X, MAX_CHARS_Y> = render::TermCharBuffer::new();

static FOOBAR: OnceCell<
    Mutex<
        BasicTermRender<
            'static,
            graphics::Simplegraphics<'static, GlobalFrameBuffer>,
            MAX_CHARS_X,
            MAX_CHARS_Y,
        >,
    >,
> = OnceCell::uninit();

pub fn init_term() {
    _ = FOO.try_init_once(|| Mutex::new(graphics::Simplegraphics::new(&GLOBAL_FRAMEBUFFER)));
    // SAFETY FOO is guaranteed to be initialized at this point. BAR is used ONLY by FOOBAR, which is only initialized once (here). This needs to be enforced here
    unsafe {
        _ = FOOBAR.try_init_once(|| {
            Mutex::new(BasicTermRender::<_, MAX_CHARS_X, MAX_CHARS_Y>::new(
                FOO.get_unchecked(),
                #[allow(static_mut_refs)]
                &mut BAR,
            ))
        });
    }
}

#[doc(hidden)]
pub fn _print(args: Arguments) {
    // SAFETY must make sure that this is not calles prior to init_term()
    unsafe {
        _ = write!(FOOBAR.get_unchecked().lock(), "{}", args);
    }
}

tests! {
    #[runner]
    fn test_buffer() {
        render::tests::test_runner();
    }
}
