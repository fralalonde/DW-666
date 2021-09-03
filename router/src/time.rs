//! RTIC Instant/Durations do not handle long enough periods
//! Define 64bits versions that handle all cases

use alloc::boxed::Box;
use crate::app::{Ticks};
use nanorand::WyRand;
use alloc::collections::VecDeque;

use embedded_time::{duration::*, Instant, Clock};

use midi::MidiError;

use crate::app;
use core::ops::Add;
use embedded_time::clock::Error;
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering::Relaxed;

#[derive(Default)]
pub struct AppClock(AtomicU32);

impl Clock for AppClock {
    type T = u32;
    const SCALING_FACTOR: Fraction = Fraction::new(1, 1000);


    fn try_now(&self) -> Result<Instant<Self>, Error> {
        Ok(self.now())
    }
}

impl AppClock {
    pub const fn new() -> Self {
        AppClock(AtomicU32::new(0))
    }

    pub fn now(&self) -> Instant<Self> {
        Instant::new(self.0.load(Relaxed))
    }

    fn tick(&self) {
        self.0.fetch_add(1, Relaxed);
    }
}

struct TimerTask {
    next_run: Instant<app::Ticks>,
    op: Box<dyn FnMut(&mut WyRand) -> Result<Option<Milliseconds>, MidiError> + Send>,
}


#[derive(Default)]
pub struct Tasks {
    new_tasks: VecDeque<TimerTask>,
}

impl Tasks {
    pub fn repeat<OP>(&mut self, task: OP)
        where OP: FnMut(&mut WyRand) -> Result<Option<Milliseconds>, MidiError> + Send + 'static
    {
        self.new_tasks.push_back(TimerTask {
            next_run: app::monotonics::now(),
            op: Box::new(task),
        })
    }

    pub fn handle(&mut self, chaos: &mut WyRand) {
        // clock tick
        app::CLOCK.tick();

        // screw priority queues (for now)
        self.new_tasks.make_contiguous().sort_by_key(|t1| t1.next_run);

        loop {
            if let Some(task) = self.new_tasks.get(0) {
                if task.next_run > app::monotonics::now() {
                    // next task not scheduled yet
                    return;
                }
            } else {
                // NO task
                return;
            }
            if let Some(mut task) = self.new_tasks.pop_front() {
                match (task.op)(chaos) {
                    Ok(Some(next_run_in)) => {
                        let now: Instant<Ticks> = app::monotonics::now();
                        let next_run = now.add(next_run_in);
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


