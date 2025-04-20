// the lexer takes all tokens until the first valid \n and passes the lexed version to the parser.
// all cursor handling, ie. backspace,... is handled by lexer.
// all commands are handeled upstream in the logic. Ie: tab - autocomplete or arrow history, ... corresponds to some Command that gets executed in the logic handler

// use crate::{
//     drivers::{
//         graphics::colors::RGBColor,
//         keyboard::{get_next, parse_scancode},
//     },
//     services::graphics::GraphicsError,
//     term::render::BasicTermRender,
// };
// use alloc::{string::String, vec::Vec};
// use core::fmt::Write;
// use embedded_graphics::prelude::DrawTarget;
// use pc_keyboard::{DecodedKey, KeyCode};

// pub(super) struct ContinuousLexer {}

// impl ContinuousLexer {
//     fn new() -> Self {
//         Self {}
//     }

//     fn get_all_next<'a, B>(&self, gfx: &mut BasicTermRender<'a, B>) -> LexingStream<'a>
//     where
//         B: DrawTarget<Color = RGBColor, Error = GraphicsError>,
//     {
//         let mut buf = String::new();
//         loop {
//             let scancode = get_next();
//             // TODO transform to key
//             let key = parse_scancode(scancode).unwrap();
//             match key {
//                 DecodedKey::Unicode(code) => match code {
//                     '\n' => {
//                         writeln!(gfx);
//                         break;
//                     }
//                     '\u{08}' => {
//                         gfx.clear_one();
//                     }
//                     _ => {}
//                 },
//                 DecodedKey::RawKey(key) => match key {
//                     KeyCode::Escape => {}
//                     KeyCode::Backspace => {}
//                     KeyCode::Tab => {}
//                     KeyCode::ArrowUp => {}
//                     KeyCode::ArrowDown => {}
//                     KeyCode::ArrowLeft => {}
//                     KeyCode::ArrowRight => {}
//                     _ => {}
//                 },
//             }
//         }
//         buf.into()
//     }
// }

// pub(super) struct LexingStream<'a> {
//     tokens: alloc::vec::IntoIter<TokenType<'a>>,
// }

// impl<'a> LexingStream<'a> {
//     fn new(tokens: alloc::vec::IntoIter<TokenType<'a>>) -> Self {
//         Self { tokens }
//     }
// }

// impl From<String> for LexingStream<'_> {
//     fn from(value: String) -> Self {
//         let mut buf = Vec::new();
//         let mut current_buf = Vec::new();
//         let mut in_qoute = false;
//         let mut chars = value.chars();
//         for c in chars {
//             match c {
//                 '\'' => {
//                     in_qoute = !in_qoute;
//                 }
//                 '\"' => {
//                     in_qoute = !in_qoute;
//                 }
//                 c if c.is_whitespace() => {}
//                 '1' => {
//                     if !in_qoute {
//                         if
//                     }
//                 }
//                 '2' => {}
//                 '>' => {
//                     if !
//                 }
//                 '|' => {}
//                 _ => {}
//             }
//         }
//         Self {
//             tokens: buf.into_iter(),
//         }
//     }
// }

// pub(super) enum TokenType<'a> {
//     Quoted(LexingStream<'a>),
//     Ident(&'a str),
//     Pipe,
//     RedirectErr(Redirect),
//     RedirectOut(Redirect),
// }

// pub(super) enum Redirect {
//     Append,
//     Overwrite,
// }
