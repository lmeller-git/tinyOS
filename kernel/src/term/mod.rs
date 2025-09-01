#![allow(dead_code)]

use core::fmt::{Arguments, Write};

use conquer_once::spin::OnceCell;
use render::BasicTermRender;

use crate::{
    drivers::graphics::{GLOBAL_FRAMEBUFFER, framebuffers::GlobalFrameBuffer},
    kernel::{devices::tty::io::read_all, threading},
    print,
    services::graphics,
    sync::locks::Mutex,
};

mod logic;
mod parse;
mod render;

// this is the max chars of the current GLOBAL_FRAMBUFFER using the current font in term/render/mod.rs
const MAX_CHARS_X: usize = 127;
const MAX_CHARS_Y: usize = 39;

// TODO clean up the mess and rewrite graphics shit
// TODO use graphics devices (maybe not to increase perf?)

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

pub fn synced_keyboard_listener() {
    let mut buf = [0; 20];
    loop {
        let n_read = read_all(&mut buf);
        for c in str::from_utf8(&buf[..n_read]).unwrap().chars() {
            print!("{c}");
        }
        threading::yield_now();
    }
}

#[doc(hidden)]
pub fn _print(args: Arguments) {
    // SAFETY must make sure that this is not calles prior to init_term()
    unsafe { _ = write!(FOOBAR.get_unchecked().lock(), "{}", args) }
}
