//! RTIC Instant/Durations do not handle long enough periods
//! Define 64bits versions that handle all cases

use cortex_m::peripheral::DWT;

#[derive(Copy, Clone, Debug)]
pub struct Instant (u64);

#[derive(Copy, Clone, Debug)]
pub struct Duration (u64);

impl core::ops::Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        if self.0 < rhs.0 {
            return Duration(0)
        }
        Duration(self.0 - rhs.0)
    }
}

impl Duration {
    pub fn millis(&self) -> u32 {
        (self.0 / crate::MILLI as u64) as u32
    }
}

/// Fuck it, let's count cycles ourselves using 64 bit.
///
/// This function needs to be called at least once every few minutes / hours to detect rollovers reliably.
/// This should not be a problem as we use it for input scanning.
// FIXME: There is possibly a more elegant way to do this whole time-since thing
pub fn long_now() -> Instant {
    static mut PREV: u32 = 0;
    static mut ROLLOVERS: u32 = 0;

    // DWT clock keeps ticking when core sleeps
    let short_now = DWT::get_cycle_count();

    Instant(unsafe {
        if short_now < PREV {
            ROLLOVERS += 1;
        }
        PREV = short_now;
        ((ROLLOVERS as u64) << 32) + short_now as u64
    })
}

// assuming that duration is never longer than u32
// pub fn short_duration(now: u32, then: u32) -> u32 {
//     if now < then {
//         now - then
//     } else {
//         // rollover detected
//         (u32::MAX - then) + now
//     }
// }