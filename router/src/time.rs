//! RTIC Instant/Durations do not handle long enough periods
//! Define 64bits versions that handle all cases

use cortex_m::peripheral::DWT;
use alloc::boxed::Box;
use crate::{CPU_FREQ};
use nanorand::WyRand;
use alloc::collections::VecDeque;
use rtic::cyccnt::{U32Ext, Duration, Instant};
use midi::MidiError;

// Just a bigger cycle counter
#[derive(Copy, Clone, Debug)]
pub struct BigInstant(pub u64);

#[derive(Copy, Clone, Debug)]
pub struct BigDuration(pub u64);

#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct Hertz(pub f32);

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

struct TimerTask {
    next_run: Instant,
    op: Box<dyn FnMut(Instant, &mut WyRand, &mut crate::tasks::Spawn) -> Result<Option<rtic::cyccnt::Duration>, MidiError> + Send>,
}

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

#[derive(Default)]
pub struct Tasks {
    new_tasks: VecDeque<TimerTask>,
    // scheduled_tasks: PriorityQueue::<TimerTask, u32, DefaultHashBuilder>,
}

impl Tasks {
    pub fn repeat<T>(&mut self, now: Instant, task: T)
        where T: FnMut(Instant, &mut WyRand, &mut crate::tasks::Spawn) -> Result<Option<rtic::cyccnt::Duration>, MidiError> + Send + 'static
    {
        self.new_tasks.push_back(TimerTask{
            next_run: now,
            op: Box::new(task)
        })
    }

    pub fn handle(&mut self, now: Instant, chaos: &mut WyRand, spawn: &mut crate::tasks::Spawn) {
        // fuck priority queues
        self.new_tasks.make_contiguous().sort_by_key(|t1| t1.next_run );

        loop {
            if let Some(task) = self.new_tasks.get(0) {
                if task.next_run > now {
                    // next task not scheduled yet
                    return
                }
            } else {
                // NO task
                return
            }
            if let Some(mut task) = self.new_tasks.pop_front() {
                match (task.op)(now, chaos, spawn) {
                    Ok(Some(next_run_in)) => {
                        let next_run = now + next_run_in;
                        if next_run <= now {
                            rprintln!("Dropping fast loop task");
                            continue;
                        }
                        task.next_run = next_run;
                        self.new_tasks.push_back(task);
                    }
                    Err(e) => {
                        rprintln!("Task error {:?}", e)
                    }
                    Ok(None) => {
                        // task finished
                    }
                }
            }
        }
    }
}


