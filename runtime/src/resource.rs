/// Async mutex.

use core::cell::{UnsafeCell};
use core::future::{poll_fn};
use core::mem::MaybeUninit;

use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use core::sync::atomic::Ordering::SeqCst;
use core::task::{Poll, Waker};

use crate::array_queue::ArrayQueue;

pub struct Local<T: Sized> {
    name: &'static str,
    init: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Sized + Send> Send for Local<T> {}

unsafe impl<T: Sized + Send> Sync for Local<T> {}

impl<T: Sized + Send> Local<T> {
    /// Create a new mutex with the given value.
    pub const fn uninit(name: &'static str) -> Self {
        Self {
            name,
            value: UnsafeCell::new(MaybeUninit::uninit()),
            init: AtomicBool::new(false),
        }
    }

    pub const fn new(name: &'static str, value: T) -> Self {
        Self {
            name,
            value: UnsafeCell::new(MaybeUninit::new(value)),
            init: AtomicBool::new(true),
        }
    }

    pub fn init_with(&self, value: T) {
        match self.init.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(false) => unsafe {
                let z = &mut (*self.value.get());
                *z.assume_init_mut() = value
            }
            err => {
                panic!("Mutex {} init twice: {}", self.name, err)
            }
        }
    }

    pub fn raw_mut(&self) -> &mut T {
        self.init_check();
        unsafe { (&mut *(self.value.get())).assume_init_mut() }
    }

    fn init_check(&self) {
        if !self.init.load(SeqCst) { panic!("Local resource {} not initialized", self.name) }
    }
}

impl<'a, T: Sized+ Send> Deref for Local<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.init_check();
        unsafe { &*(self.value.get() as *const T) }
    }
}

impl<'a, T: Sized+ Send> DerefMut for Local<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.init_check();
        unsafe { (&mut *(self.value.get())).assume_init_mut() }
    }
}

pub struct Shared<T: Sized> {
    name: &'static str,
    init: AtomicBool,
    locked: AtomicBool,
    wake_queue: ArrayQueue<Waker, 2>,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Sized + Send> Send for Shared<T> {}

unsafe impl<T: Sized + Send> Sync for Shared<T> {}

impl<T: Sized> Shared<T> {
    /// Create a new mutex with the given value.
    pub const fn uninit(name: &'static str) -> Self {
        Self {
            name,
            value: UnsafeCell::new(MaybeUninit::uninit()),
            init: AtomicBool::new(false),
            locked: AtomicBool::new(false),
            wake_queue: ArrayQueue::new(),
        }
    }
    pub fn init_with(&self, value: T) {
        match self.init.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(false) => unsafe {
                let z = &mut (*self.value.get());
                *z.assume_init_mut() = value
            }
            err => {
                panic!("Mutex {} init twice: {}", self.name, err)
            }
        }
    }

    pub async fn lock(&self) -> SharedGuard<'_, T> {
        poll_fn(|cx| {
            if let Ok(false) = self.locked.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
                Poll::Ready(SharedGuard { mutex: self })
            } else {
                if self.wake_queue.push(cx.waker().clone()).is_err() {
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
            if let Some(waker) = self.mutex.wake_queue.pop() {
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
