/// Async mutex.

use core::cell::{UnsafeCell};
use core::future::{poll_fn};
use core::mem::MaybeUninit;

use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Poll, Waker};
use crate::pri_queue::PriorityQueue;
use crate::{SpinMutex};

pub struct Local<T: Sized> {
    init: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Sized + Send> Send for Local<T> {}

unsafe impl<T: Sized + Send> Sync for Local<T> {}

impl<T: Sized + Send> Local<T> {
    /// Create a new mutex with the given value.
    pub const fn uninit() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            init: AtomicBool::new(false),
        }
    }

    pub fn init_with(&self, name: &'static str, value: T) {
        match self.init.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(false) => unsafe {
                let z = &mut (*self.value.get());
                *z.assume_init_mut() = value
            }
            err => {
                panic!("Mutex {} init twice: {}", name, err)
            }
        }
    }
}

impl<'a, T: Sized, > Deref for Local<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // If you already have the guard, you have access
        unsafe { &*(self.value.get() as *const T) }
    }
}

impl<'a, T: Sized, > DerefMut for Local<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // If you already have the guard, you have access
        unsafe { (&mut *(self.value.get())).assume_init_mut() }
    }
}

pub struct Shared<T: Sized> {
    init: AtomicBool,
    locked: AtomicBool,
    wake_queue: SpinMutex<PriorityQueue<u8, Waker, 2>>,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Sized + Send> Send for Shared<T> {}

unsafe impl<T: Sized + Send> Sync for Shared<T> {}

impl<T: Sized> Shared<T> {
    /// Create a new mutex with the given value.
    pub const fn uninit() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            init: AtomicBool::new(false),
            locked: AtomicBool::new(false),
            wake_queue: SpinMutex::new(PriorityQueue::new()),
        }
    }
    pub fn init_with(&self, name: &'static str, value: T) {
        match self.init.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(false) => unsafe {
                let z = &mut (*self.value.get());
                *z.assume_init_mut() = value
            }
            err => {
                panic!("Mutex {} init twice: {}", name, err)
            }
        }
    }

    pub async fn lock(&self) -> SharedGuard<'_, T> {
        poll_fn(|cx| {
            if let Ok(false) = self.locked.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
                Poll::Ready(SharedGuard { mutex: self })
            } else {
                if !self.wake_queue.lock().push(1, cx.waker()) {
                    panic!("wake queue overflow")
                }
                Poll::Pending
            }
        }).await
    }
}

/// Async mutex guard.
///
/// Owning an instance of this type indicates having
/// successfully locked the mutex, and grants access to the contents.
///
/// Dropping it unlocks the mutex.
pub struct SharedGuard<'a, T: Sized, > {
    mutex: &'a Shared<T>,
}

impl<'a, T: Sized> Drop for SharedGuard<'a, T> {
    fn drop(&mut self) {
        if let Ok(true) = self.mutex.locked.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst) {
            // only the previous owner of the lock may wake a pending owner (if any)
            if let Some(waker) = self.mutex.wake_queue.lock().pop() {
                waker.wake();
            }
        }
    }
}

impl<'a, T: Sized, > Deref for SharedGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // If you already have the guard, you have access
        unsafe { &*(self.mutex.value.get() as *const T) }
    }
}

impl<'a, T: Sized, > DerefMut for SharedGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // If you already have the guard, you have access
        unsafe { (&mut *(self.mutex.value.get())).assume_init_mut() }
    }
}
