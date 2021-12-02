#![no_std]

extern crate alloc;

#[macro_use]
extern crate log;

mod time;
mod exec;

pub use time::{now, now_millis, delay_until, delay_us, delay_ms, run_scheduled};
pub use exec::{spawn, process_queue};

pub fn init() {
    time::init();
    exec::init();
}