// NOTE waker logic is based on async-std v1.5.0

use core::{
    cell::{Cell, UnsafeCell},
    future::Future,
    ops,
    pin::Pin,
    task::{Context, Poll},
};

use cortex_m::asm;
use slotmap::DefaultKey;

use super::waker_set::WakerSet;

/// A mutual exclusion primitive for protecting shared data
pub struct AsyncMutex<T> {
    locked: Cell<bool>,
    value: UnsafeCell<T>,
    wakers: WakerSet,
}

unsafe impl <T> Send for AsyncMutex<T> {
    
}

unsafe impl <T> Sync for AsyncMutex<T> {

}

impl<T> AsyncMutex<T> {
    /// Creates a new mutex
    pub fn new(t: T) -> Self {
        Self {
            locked: Cell::new(false),
            wakers: WakerSet::new(),
            value: UnsafeCell::new(t),
        }
    }

    /// Acquires the lock
    ///
    /// Returns a guard that release the lock when dropped
    pub async fn lock(&self) -> AsyncMutexGuard<'_, T> {
        struct Lock<'a, T> {
            mutex: &'a AsyncMutex<T>,
            opt_key: Option<DefaultKey>,
        }

        impl<'a, T> Future for Lock<'a, T> {
            type Output = AsyncMutexGuard<'a, T>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                // If the current task is in the set, remove it.
                if let Some(key) = self.opt_key.take() {
                    self.mutex.wakers.remove(key);
                }

                // Try acquiring the lock.
                match self.mutex.try_lock() {
                    Some(guard) => Poll::Ready(guard),
                    None => {
                        // Insert this lock operation.
                        self.opt_key = Some(self.mutex.wakers.insert(cx));

                        Poll::Pending
                    }
                }
            }
        }

        impl<T> Drop for Lock<'_, T> {
            fn drop(&mut self) {
                // If the current task is still in the set, that means it is being cancelled now.
                if let Some(key) = self.opt_key {
                    self.mutex.wakers.cancel(key);
                }
            }
        }

        Lock {
            mutex: self,
            opt_key: None,
        }
        .await
    }

    /// Attempts to acquire the lock
    pub fn try_lock(&self) -> Option<AsyncMutexGuard<'_, T>> {
        if !self.locked.get() {
            self.locked.set(true);
            Some(AsyncMutexGuard(self))
        } else {
            None
        }
    }
}

/// A guard that releases the lock when dropped
pub struct AsyncMutexGuard<'a, T>(&'a AsyncMutex<T>);

impl<T> Drop for AsyncMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.0.locked.set(false);
        self.0.wakers.notify_any();
        asm::wfe();
    }
}

impl<T> ops::Deref for AsyncMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.0.value.get() }
    }
}

impl<T> ops::DerefMut for AsyncMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.0.value.get() }
    }
}
