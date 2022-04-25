use alloc::boxed::Box;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use woke::{waker_ref, Woke};
use crate::array_queue::ArrayQueue;
use crate::resource::Local;

use crate::SpinMutex;

static REACTOR: Local<Reactor> = Local::uninit("REACTOR");

pub fn init() {
    REACTOR.init_with(Reactor::new());
}

struct Task {
    pub future: SpinMutex<Option<Pin<Box<dyn Future<Output=()> + Send + 'static>>>>,
}

pub struct Reactor {
    exec_queue:ArrayQueue<Arc<Task>, 64>,
}

impl Woke for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let cloned = arc_self.clone();
        REACTOR.enqueue(&cloned);
    }
}

pub fn spawn(future: impl Future<Output=()> + 'static + Send) {
    let future = Box::pin(future);
    let task = Arc::new(Task {
        future: SpinMutex::new(Some(future)),
    });
    REACTOR.enqueue(&task)
}

pub fn process_queue() {
    REACTOR.process()
}


impl Reactor {
    pub fn new() -> Self {
        Self {
            exec_queue: ArrayQueue::new(),
        }
    }

    fn enqueue(&self, task: &Arc<Task>) {
        if self.exec_queue.push(task.clone()).is_err() {
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
