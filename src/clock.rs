//! RTIC Instant/Durations do not handle long enough periods
//! Define 64bits versions that handle all cases

use cortex_m::peripheral::DWT;
use hashbrown::HashMap;
use alloc::boxed::Box;
use crate::{Handle, NEXT_HANDLE, CPU_FREQ};
use core::sync::atomic::Ordering::Relaxed;
use nanorand::WyRand;
use alloc::collections::VecDeque;
use rtic::cyccnt::{U32Ext, Duration};
use spin::mutex::spin::SpinMutex;
use alloc::sync::Arc;

#[derive(Copy, Clone, Debug)]
pub struct BigInstant(u64);

#[derive(Copy, Clone, Debug)]
pub struct BigDuration(u64);

impl core::ops::Sub for BigInstant {
    type Output = BigDuration;

    fn sub(self, rhs: Self) -> Self::Output {
        if self.0 < rhs.0 {
            return BigDuration(0);
        }
        BigDuration(self.0 - rhs.0)
    }
}

impl BigDuration {
    pub fn millis(&self) -> u32 {
        (self.0 / crate::CYCLES_PER_MILLISEC as u64) as u32
    }
}

/// Fuck it, let's count cycles ourselves using 64 bit.
///
/// This function needs to be called at least once every few minutes / hours to detect rollovers reliably.
/// This should not be a problem as we use it for input scanning.
// FIXME: There is possibly a more elegant way to do this whole time-since thing
pub fn long_now() -> BigInstant {
    static mut PREV: u32 = 0;
    static mut ROLLOVERS: u32 = 0;

    // DWT clock keeps ticking when core sleeps
    let short_now = DWT::get_cycle_count();

    BigInstant(unsafe {
        if short_now < PREV {
            ROLLOVERS += 1;
        }
        PREV = short_now;
        ((ROLLOVERS as u64) << 32) + short_now as u64
    })
}

pub type TimerTask = Box<dyn FnMut(&mut crate::timer_task::Resources, &mut crate::timer_task::Spawn) -> Option<rtic::cyccnt::Duration> + Send>;

pub trait TimeUnits {
    fn millis(&self) -> Duration;
    fn micros(&self) -> Duration;
}

pub const CYCLES_PER_MICROSEC: u32 = CPU_FREQ / 1_000_000;
pub const CYCLES_PER_MILLISEC: u32 = CPU_FREQ / 1_000;

impl TimeUnits for u32 {
    fn millis(&self) -> Duration {
        (self * CYCLES_PER_MILLISEC).cycles()
    }

    fn micros(&self) -> Duration {
        (self * CYCLES_PER_MICROSEC).cycles()
    }
}


