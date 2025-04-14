#[cfg(not(feature = "std"))]
use ::core::fmt::Arguments;
#[cfg(feature = "std")]
use ::std::fmt::Arguments;

pub trait Logger: Sync + Send {
    fn log(&self, msg: Arguments);
}

static mut LOGGER: Option<&'static dyn Logger> = None;

pub fn set_logger(logger: &'static dyn Logger) {
    // TODO implement OnceCell (or similar) based logic
    // SAFETY this is safe, since it panics on overwrite
    #[allow(static_mut_refs)]
    if unsafe { LOGGER.is_some() } {
        panic!("Logger already set");
    }
    unsafe { LOGGER = Some(logger) }
}

pub fn log(args: Arguments) {
    unsafe {
        if let Some(logger) = LOGGER {
            logger.log(args);
        }
    }
}
