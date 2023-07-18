use alloc::sync::Arc;
use core::fmt::{Formatter, Pointer};
use core::future::Future;
// use core::mem::MaybeUninit;
use core::pin::Pin;
use core::sync::atomic::AtomicU32;

use core::sync::atomic::Ordering::Relaxed;
use core::task::{Context, Poll, Waker};

use cortex_m::peripheral::{SYST};
use cortex_m::peripheral::syst::SystClkSource;

// use embedded_time::clock::Error;
// use embedded_time::fraction::Fraction;
// use embedded_time::{Clock, Instant};
// use embedded_time::duration::{Microseconds, Milliseconds, Nanoseconds};

use crate::pri_queue::PriorityQueue;

use cortex_m_rt::exception;
use fugit::{Duration, Instant};

use crate::{Local, SpinMutex};
use crate::RuntimeError;

const SYSTICK_CYCLES: u32 = 96_000_000;

pub type SysInstant = Instant<u64, 1, SYSTICK_CYCLES>;
pub type SysDuration = Duration<u32, 1, SYSTICK_CYCLES>;

pub struct SysClock {
    syst: &'static mut SYST,
    past_cycles: AtomicU32,
}

impl core::fmt::Debug for SysClock {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        self.syst.fmt(f)
    }
}

static CLOCK: Local<SysClock> = Local::uninit("CLOCK");

pub fn init(syst: &'static mut SYST) {
    CLOCK.init_static(SysClock::new(syst));
}

pub fn now() -> SysInstant {
    CLOCK.now()
}

pub fn later(cycles: u64) -> SysInstant {
    CLOCK.later(cycles)
}

pub fn now_millis() -> u64 {
    (now() - SysClock::zero()).to_millis()
}

const MAX_RVR: u32 = 0x00FF_FFFF;

impl SysClock {
    fn new(syst: &'static mut SYST) -> Self {
        syst.disable_interrupt();
        syst.disable_counter();
        syst.clear_current();

        syst.set_clock_source(SystClkSource::Core);
        syst.set_reload(MAX_RVR);

        syst.enable_counter();

        // actually enables the #[exception] SysTick (see below)
        syst.enable_interrupt();

        Self {
            syst,
            past_cycles: AtomicU32::new(0),
        }
    }

    fn zero() -> SysInstant {
        SysInstant::from_ticks(0)
    }

    fn now(&self) -> SysInstant {
        SysInstant::from_ticks(self.cycles())
    }

    fn later(&self, period: u64) -> SysInstant {
        SysInstant::from_ticks(self.cycles() + period)
    }

    #[inline]
    fn cycles(&self) -> u64 {
        // systick cvr counts DOWN
        let elapsed_cycles = MAX_RVR - self.syst.cvr.read();
        self.past_cycles.load(Relaxed) as u64 + elapsed_cycles as u64
    }

    #[inline]
    pub fn rollover(&self) {
        self.past_cycles.fetch_add(MAX_RVR, Relaxed);
    }
}

#[exception]
fn SysTick() {
    CLOCK.rollover()
}

// impl<const FREQ: u32> Clock for SysClock<FREQ> {
//     type T = u64;
//
//     const SCALING_FACTOR: Fraction = Fraction::new(1, FREQ);
//
//     fn try_now(&self) -> Result<SysInstant, Error> {
//         Ok(self.now())
//     }
// }

static SCHED: SpinMutex<PriorityQueue<SysInstant, Arc<dyn Fn(SysInstant) + 'static + Send + Sync>, 16>> = SpinMutex::new(PriorityQueue::new());

pub fn schedule_at<F>(when: SysInstant, what: F)
    where F: Fn(SysInstant) + 'static + Send + Sync,
{
    let mut sched = SCHED.lock();
    let f: Arc<dyn Fn(SysInstant) + 'static + Send + Sync> = Arc::new(what);
    if !sched.push(when, &f) {
        panic!("No scheduler slot left")
    }
}

pub fn run_scheduled() {
    let mut sched = SCHED.lock();
    while let Some((due_time, wake_fn)) = sched.pop_due(now()) {
        wake_fn(due_time)
    }
}

pub fn delay(duration: SysDuration) -> AsyncDelay {
    let due_time = now() + duration;
    delay_until(due_time)
}

// pub fn delay_us(duration: u32) -> AsyncDelay {
//     let due_time = now() + Microseconds(duration);
//     delay_until(due_time)
// }
//
// pub fn delay_ns(duration: u32) -> AsyncDelay {
//     let due_time = now() + Nanoseconds(duration);
//     delay_until(due_time)
// }

pub fn delay_cycles(duration: u64) -> AsyncDelay {
    let due_time = later(duration);
    delay_until(due_time)
}

pub fn delay_until(due_time: SysInstant) -> AsyncDelay {
    let waker: Arc<SpinMutex<Option<Waker>>> = Arc::new(SpinMutex::new(None));
    let sched_waker = waker.clone();
    schedule_at(due_time, move |time| {
        if let Some(waker) = sched_waker.lock().take() {
            waker.wake()
        }
    });
    AsyncDelay { waker, due_time }
}

pub struct AsyncDelay {
    waker: Arc<SpinMutex<Option<Waker>>>,
    due_time: SysInstant,
}

impl Future for AsyncDelay {
    type Output = Result<(), RuntimeError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = now();
        if self.due_time <= now {
            Poll::Ready(Ok(()))
        } else {
            let mut waker = self.waker.lock();
            *waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}