#![feature(alloc_error_handler)]
#![feature(future_poll_fn)]

#![no_std]

extern crate alloc;

extern crate spin as spin_mmut;

#[macro_use]
extern crate defmt;

mod time;
mod exec;
mod array_queue;
mod resource;

use cortex_m::peripheral::SYST;
pub use time::{now, now_millis, delay_until, delay, delay_cycles, run_scheduled, SysInstant};
pub use exec::{spawn, /*repeat,*/ process_queue};
pub use spin::{Mutex as SpinMutex, MutexGuard as SpinMutexGuard};

pub use fugit::{ExtU32, Instant, Duration};

pub mod log_defmt;

pub use defmt::{debug, info, warn, error, trace};
pub use resource::{Shared, SharedGuard, Local};

mod pri_queue;

pub mod allocator;

pub fn init(syst: &'static mut SYST) {
    time::init(syst);
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