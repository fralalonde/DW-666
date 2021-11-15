use alloc::boxed::Box;
use core::fmt::Formatter;
use core::future::Future;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use embedded_time::duration::Duration;

use sync_thumbv6m::alloc::Arc;
use sync_thumbv6m::array_queue::ArrayQueue;
use woke::{waker_ref, Woke};

use sync_thumbv6m::spin::SpinMutex;

struct Task {
    pub future: SpinMutex<Option<Pin<Box<dyn Future<Output=()> + Send + 'static>>>>,
    pub reactor: Arc<Reactor>,
}

impl core::fmt::Debug for Task {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        // todo!()
        Ok(())
    }
}

pub struct Reactor {
    queue: Arc<ArrayQueue<Arc<Task>>>,
    clock: Arc<fn() -> Instant>
}


impl Woke for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let cloned = arc_self.clone();
        arc_self.reactor.enqueue(cloned);
    }
}

impl Reactor {
    pub fn new(size: usize, clock: fn() -> Instant) -> Self {
        Self {
            queue: Arc::new(ArrayQueue::new(size)),
            clock: Arc::new(clock),
        }
    }

    pub fn spawn(self: Arc<Self>, future: impl Future<Output=()> + 'static + Send) {
        let future = Box::pin(future);
        let task = Arc::new(Task {
            future: SpinMutex::new(Some(future)),
            reactor: self.clone(),
        });
        self.enqueue(task)
    }

    fn enqueue(&self, task: Arc<Task>) {
        self.queue.push(task).unwrap()
    }

    pub fn advance(&self) {
        while let Some(task) = self.queue.pop() {
            let mut future_slot = task.future.lock();
            if let Some(mut future) = future_slot.take() {
                let waker = waker_ref(&task);
                let context = &mut Context::from_waker(&*waker);
                if let Poll::Pending = future.as_mut().poll(context) {
                    // put it back
                    *future_slot = Some(future);
                }
            }
        }
    }
}

pub type Instant = usize;

pub struct AsyncDelay {
    scheduled: Arc<SpinMutex<Scheduled>>,
    clock: Arc<fn() -> Instant>,
}

impl AsyncDelay {
    pub fn new(duration: impl Duration, clock: Arc<fn() -> Instant>) -> Self {
        let scheduled = Arc::new(SpinMutex::new(Scheduled {
            wake_at: 0,
            waker: None,
        }));

        // TODO insert wake_sched in timer PQ
        // let sched2 = schedule.clone();
        // thread::spawn(move || {
        //     thread::sleep(duration);
        //     let mut shared_state = sched2.lock().unwrap();
        //     shared_state.completed = true;
        //     if let Some(waker) = shared_state.waker.take() {
        //         waker.wake()
        //     }
        // });

        AsyncDelay { scheduled, clock }
    }
}


impl Future for AsyncDelay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut timer = self.scheduled.lock();
        if timer.wake_at <= (self.clock)() {
            Poll::Ready(())
        } else {
            timer.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

struct Scheduled {
    wake_at: Instant,
    waker: Option<Waker>,
}

