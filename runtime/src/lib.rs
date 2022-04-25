#![feature(alloc_error_handler)]
#![feature(future_poll_fn)]
#![feature(generic_associated_types)]

#![no_std]

extern crate alloc;

extern crate spin as spin_mmut;

#[macro_use]
extern crate defmt;

mod time;
mod exec;
// mod spin_mutex;
mod array_queue;
mod relax;
// mod shared;
mod mutex;

pub use time::{now, now_millis, delay_until, delay_us, delay_ms, delay_ns, delay_cycles, run_scheduled};
pub use exec::{spawn, process_queue};
pub use spin::{Mutex as SpinMutex, MutexGuard as SpinMutexGuard};

pub mod log_defmt;
pub use defmt::{debug, info, warn, error, trace};
// pub use shared::Shared;
pub use mutex::{Shared, SharedGuard};

mod pri_queue;

pub mod cxalloc;

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

#[derive(Copy, Clone, Debug, PartialEq, defmt::Format)]
pub enum RuntimeError {
    Interrupted,
}