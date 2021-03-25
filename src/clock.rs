use cortex_m::peripheral::DWT;

pub const CPU_FREQ: u32 = 100_000_000;
pub const PCLK1_FREQ: u32 = CPU_FREQ / 2;

/// regular RTIC Instant/Durations sucks for real time, does not handle rollovers
/// (and STM32F1 RTC is in seconds only)
/// Fuck it, let's count cycles ourselves using 64 bit.
///
/// This function needs to be called at least once every few minutes / hours to detect rollovers reliably.
/// This should not be a problem as we use it for input scanning.
// FIXME: There is possibly a more elegant way to do this whole time-since thing
pub fn long_now() -> u64 {
    static mut PREV: u32 = 0;
    static mut ROLLOVERS: u32 = 0;

    // using DWT clock because it keeps ticking even when core sleeps (?)
    let short_now = DWT::get_cycle_count();

    unsafe {
        if short_now < PREV {
            ROLLOVERS += 1;
        }
        PREV = short_now;
        ((ROLLOVERS as u64) << 32) + short_now as u64
    }
}

/// assuming that duration is never longer than u32
pub fn short_duration(now: u32, then: u32) -> u32 {
    if now < then {
        now - then
    } else {
        // rollover detected
        (u32::MAX - then) + now
    }
}