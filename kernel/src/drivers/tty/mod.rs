use crate::kernel::{
    devices::tty::{
        TTYSink,
        sink::{FBBACKEND, SERIALBACKEND},
    },
    threading,
};

//TODO add wake up logic
pub fn start_tty_backend() {
    _ = threading::spawn(move || {
        loop {
            SERIALBACKEND.get().unwrap().flush();
            FBBACKEND.get().unwrap().flush();
            threading::yield_now();
        }
    })
    .unwrap();
}
