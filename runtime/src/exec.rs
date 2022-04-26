use alloc::boxed::Box;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use embedded_time::{Instant };
use embedded_time::duration::Duration;
use embedded_time::fixed_point::FixedPoint;

use woke::{waker_ref, Woke};
use crate::array_queue::ArrayQueue;
use crate::resource::Local;

use crate::{now, SpinMutex, time};
use crate::time::{SysInstant, SysTickClock};

static REACTOR: Local<Reactor> = Local::uninit("REACTOR");

pub fn init() {
    REACTOR.init_with(Reactor::new());
}

struct Task {
    pub future: SpinMutex<Option<Pin<Box<dyn Future<Output=()> + Send + 'static>>>>,
}

const MAX_PENDING_TASK: usize = 64;

pub struct Reactor {
    exec_queue:ArrayQueue<Arc<Task>, MAX_PENDING_TASK>,
}

impl Woke for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let cloned = arc_self.clone();
        REACTOR.enqueue(cloned);
    }
}

pub fn spawn(future: impl Future<Output=()> + 'static + Send) {
    let future = Box::pin(future);
    let task = Arc::new(Task {
        future: SpinMutex::new(Some(future)),
    });
    REACTOR.enqueue(task)
}

// pub fn repeat(every: impl Duration, fun: impl FnMut(SysInstant) + 'static + Send) {
//     time::schedule_at(now(), |now| {
//         fun(now);
//         time::schedule_at(now + every, fun);
//     })
// }

pub fn process_queue() {
    REACTOR.process()
}

// see https://github.com/rust-lang/rust/issues/44796
const INIT_TASK: Option<Arc<Task>> = None;

impl Reactor {
    pub fn new() -> Self {
        Self {
            exec_queue: ArrayQueue::new([INIT_TASK; MAX_PENDING_TASK]),
        }
    }

    fn enqueue(&self, task: Arc<Task>) {
        if self.exec_queue.push(task).is_err() {
            warn!("Reactor queue full - is a task blocking?")
        }
    }

    pub fn process(&self) {
        if let Some(task) = self.exec_queue.pop() {
            let mut task_future = task.future.lock();
            if let Some(mut future) = task_future.take() {
                let waker = waker_ref(&task);
                let context = &mut Context::from_waker(&*waker);
                if let Poll::Pending = future.as_mut().poll(context) {
                    *task_future = Some(future);
                }
            } else {
                warn!("NO FUTURE")
            }
        }
    }
}
