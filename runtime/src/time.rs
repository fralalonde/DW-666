use core::fmt::{Formatter, Pointer};
use core::future::Future;
use core::mem::MaybeUninit;
use core::pin::Pin;

use core::sync::atomic::Ordering::Relaxed;
use core::task::{Context, Poll, Waker};

use atomic_polyfill::{AtomicU64};

use cortex_m::peripheral::{SYST};
use cortex_m::peripheral::syst::SystClkSource;

use embedded_time::clock::Error;
use embedded_time::fraction::Fraction;
use embedded_time::{Clock, Instant};
use embedded_time::duration::{ Microseconds, Milliseconds};

use sync_thumbv6m::spin::SpinMutex;
use sync_thumbv6m::pri_queue::PriorityQueue;

use cortex_m_rt::exception;
use sync_thumbv6m::alloc::Arc;

pub struct SysTickClock<const FREQ: u32> {
    systick: &'static mut SYST,
    past_cycles: AtomicU64,
}

impl<const FREQ: u32> core::fmt::Debug for SysTickClock<FREQ> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        self.systick.fmt(f)
    }
}

// const MAX_SYSTICK_CYCLES: u32 = 0x00ffffff;
const SYSTICK_CYCLES: u32 = 48_000_000;

pub const SYST_FREQ: u32 = 48_000_000;

static mut CLOCK: MaybeUninit<SysTickClock<SYST_FREQ>> = MaybeUninit::uninit();

pub fn init() {
    unsafe { CLOCK = MaybeUninit::new(SysTickClock::new()) };
}

pub fn now() -> Instant<SysTickClock<SYST_FREQ>> {
    unsafe { CLOCK.assume_init_ref().now() }
}

pub fn now_millis() -> u64 {
    Milliseconds::try_from(now() - SysTickClock::zero()).unwrap().0
}

// fuck it
#[allow(mutable_transmutes)]
fn syst() -> &'static mut SYST {
    unsafe { core::mem::transmute(&*SYST::ptr()) }
}

impl<const FREQ: u32> SysTickClock<FREQ> {
    fn new() -> Self {
        let systick = syst();
        systick.disable_interrupt();
        systick.disable_counter();
        systick.clear_current();

        systick.set_clock_source(SystClkSource::Core);
        systick.set_reload(SYSTICK_CYCLES - 1);

        systick.enable_counter();
        systick.enable_interrupt();

        Self {
            systick,
            past_cycles: AtomicU64::new(0),
        }
    }

    fn zero() -> Instant<Self> {
        Instant::new(0)
    }

    fn now(&self) -> Instant<Self> {
        Instant::new(self.cycles())
    }

    fn cycles(&self) -> u64 {
        // systick counts DOWN
        let elapsed_cycles = SYSTICK_CYCLES - self.systick.cvr.read();
        self.past_cycles.load(Relaxed) + elapsed_cycles as u64
    }

    pub fn rollover(&self) {
        self.past_cycles.fetch_add(SYSTICK_CYCLES as u64, Relaxed);
    }
}

#[exception]
fn SysTick() {
    unsafe { CLOCK.assume_init_ref().rollover() };
}

impl<const FREQ: u32> Clock for SysTickClock<FREQ> {
    type T = u64;

    const SCALING_FACTOR: Fraction = Fraction::new(1, FREQ);

    fn try_now(&self) -> Result<Instant<Self>, Error> {
        Ok(self.now())
    }
}

static SCHED: SpinMutex<PriorityQueue<Instant<SysTickClock<SYST_FREQ>>, Arc<dyn Fn() + 'static + Send + Sync>, 16>> = SpinMutex::new(PriorityQueue::new());

pub fn schedule_at<F: Fn() + 'static + Send + Sync>(when: Instant<SysTickClock<SYST_FREQ>>, what: F) {
    let mut sched = SCHED.lock();
    let f: Arc<dyn Fn() + 'static + Send + Sync> = Arc::new(what);
    if !sched.push(when, &f) {
        panic!("No scheduler slot left")
    }
}

pub fn run_scheduled() /*-> Option<Arc<dyn Fn() + Sync + Send + 'static>>*/ {
    let now = crate::time::now();
    let mut sched = SCHED.lock();
    while let Some(due) = sched.pop_due(now) {
        due()
    }
}

pub fn delay_ms(duration: u32) -> AsyncDelay {
    let due_time = now() + Milliseconds(duration);
    delay_until(due_time)
}

pub fn delay_us(duration: u32) -> AsyncDelay {
    let due_time = now() + Microseconds(duration);
    delay_until(due_time)
}

pub fn delay_until(due_time: Instant<SysTickClock<SYST_FREQ>>) -> AsyncDelay {
    let waker: Arc<SpinMutex<Option<Waker>>> = Arc::new(SpinMutex::new(None));
    let sched_waker = waker.clone();
    schedule_at(due_time, move || {
        if let Some(waker) = sched_waker.lock().take() {
            waker.wake()
        }
    });
    AsyncDelay { waker, due_time }
}

pub struct AsyncDelay {
    waker: Arc<SpinMutex<Option<Waker>>>,
    due_time: Instant<SysTickClock<SYST_FREQ>>,
}

impl Future for AsyncDelay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = now();
        if self.due_time <= now {
            Poll::Ready(())
        } else {
            let mut waker = self.waker.lock();
            *waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}