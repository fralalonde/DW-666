#![feature(const_fn_trait_bound)]

#![no_std]

extern crate alloc;

mod time;
mod exec;

pub use time::{now, now_millis, delay_until, delay_us, delay_ms, run_scheduled};
pub use exec::{spawn, process_queue};

// #[macro_use]
// extern crate defmt;
//
// pub use defmt::{debug, trace, info, warn, error, assert, assert_eq, assert_ne, debug_assert, timestamp, panic, todo, unwrap, unreachable, unimplemented};
// mod log_defmt;

pub use log::{debug, info, warn, error, trace};

pub mod log_rtt;
// pub mod log_serial;

mod pri_queue;

pub fn init() {
    time::init();
    debug!("time ok");

    exec::init();
    debug!("exec ok");
}

/// Terminates the application and makes `probe-run` exit with exit-code = 0
pub fn exit() -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}
