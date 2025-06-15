// #[macro_export]
// macro_rules! println {
//     () => ($crate::print!("\n"));
//     ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
// }

// #[macro_export]
// macro_rules! print {
//     () => {};
//     ($($arg:tt)*) => ($crate::term::_print(format_args!($($arg)*)));

// }

// #[macro_export]
// macro_rules! cross_println {
//     () => ($crate::cross_print!("\n"));
//     ($($arg:tt)*) => ($crate::cross_print!("{}\n", format_args!($($arg)*)));
// }

// #[macro_export]
// macro_rules! cross_print {
//     () => {};
//     ($($arg:tt)*) => ({
//         $crate::term::_print(format_args!($($arg)*));
//         $crate::serial_print!("{}", format_args!($($arg)*));
//     });

// }
