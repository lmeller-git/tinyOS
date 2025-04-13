#![cfg_attr(not(feature = "std"), no_std)]

pub mod logging;
pub mod testing;
pub mod utils;

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::logging::log(format_args!($($arg)*));
    };
}

#[cfg(test)]
mod tests {}
