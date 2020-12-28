/// regular RTIC Instant/Durations sucks for real time, does not handle rollovers
/// let's count cycles ourselves using 64 bit!
/// this needs to be called at least once every few minutes / hours to detect rollovers reliably
/// which should not be a problem if used for input scanning
pub fn long_now(short_now: u32) -> u64 {
    static mut PREV: u32 = 0;
    static mut ROLLOVERS: u32 = 0;

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