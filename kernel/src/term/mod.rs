#![allow(dead_code)]

use crate::{
    arch,
    drivers::{
        graphics::{GLOBAL_FRAMEBUFFER, framebuffers::GlobalFrameBuffer},
        keyboard::{KEYBOARD_BUFFER, parse_scancode},
    },
    locks::primitive::Mutex,
    print,
    services::graphics,
};
use conquer_once::spin::OnceCell;
use core::fmt::{Arguments, Write};
use os_macros::tests;
use render::BasicTermRender;

mod logic;
mod parse;
mod render;

// this is the max chars of the current GLOBAL_FRAMBUFFER using the current font in term/render/mod.rs
const MAX_CHARS_X: usize = 127;
const MAX_CHARS_Y: usize = 39;

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

pub fn synced_keyboard_listener() {
    // serial_println!("{:#?}", *FOOBAR.get().unwrap().lock());
    loop {
        // serial_println!("w");
        if let Ok(v) = KEYBOARD_BUFFER.pop() {
            if let Ok(res) = parse_scancode(v) {
                // serial_println!("{:#?}", res);
                match res {
                    pc_keyboard::DecodedKey::RawKey(_k) => {}
                    pc_keyboard::DecodedKey::Unicode(c) => match c {
                        '\u{08}' => unsafe {
                            FOOBAR.get_unchecked().lock().clear_one();
                        },
                        _ => print!("{}", c),
                    },
                }
            }
        }
        crate::arch::hlt();
    }
}

#[doc(hidden)]
pub fn _print(args: Arguments) {
    // SAFETY must make sure that this is not calles prior to init_term()
    unsafe { _ = write!(FOOBAR.get_unchecked().lock(), "{}", args) }
}
