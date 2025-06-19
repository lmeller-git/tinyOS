use crate::{
    kernel::{
        devices::tty::{
            TTYSink,
            sink::{FBBACKEND, SERIALBACKEND},
        },
        threading,
    },
    locks::GKL,
    serial_println,
};

//TODO add wake up logic
pub fn start_tty_backend() {
    _ = threading::spawn(move || {
        loop {
            let Ok(gkl) = GKL.try_lock() else {
                threading::yield_now();
                continue;
            };
            SERIALBACKEND.get().unwrap().flush();
            FBBACKEND.get().unwrap().flush();
            drop(gkl);
            threading::yield_now();
        }
    })
    .unwrap();
}
