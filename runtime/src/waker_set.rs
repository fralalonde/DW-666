// NOTE based on async-std v1.5.0

use core::{
    cell::UnsafeCell,
    task::{Context, Waker},
};

use slotmap::{SlotMap, DefaultKey};

// NOTE this should only ever be used in "Thread mode"
pub struct WakerSet {
    inner: UnsafeCell<Inner>,
}

impl WakerSet {
    pub fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Inner::new()),
        }
    }

    pub fn cancel(&self, key: DefaultKey) -> bool {
        // NOTE(unsafe) single-threaded context; OK as long as no references are returned
        unsafe { (*self.inner.get()).cancel(key) }
    }

    pub fn notify_any(&self) -> bool {
        // NOTE(unsafe) single-threaded context; OK as long as no references are returned
        unsafe { (*self.inner.get()).notify_any() }
    }

    pub fn notify_one(&self) -> bool {
        // NOTE(unsafe) single-threaded context; OK as long as no references are returned
        unsafe { (*self.inner.get()).notify_one() }
    }

    pub fn insert(&self, cx: &Context<'_>) -> DefaultKey {
        // NOTE(unsafe) single-threaded context; OK as long as no references are returned
        unsafe { (*self.inner.get()).insert(cx) }
    }

    pub fn remove(&self, key: DefaultKey) {
        // NOTE(unsafe) single-threaded context; OK as long as no references are returned
        unsafe { (*self.inner.get()).remove(key) }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Notify {
    /// Make sure at least one entry is notified.
    Any,
    /// Notify one additional entry.
    One,
    // Notify all entries.
    // All,
}

struct Inner {
    // NOTE the number of entries is capped at `NTASKS`
    entries: SlotMap<DefaultKey, Option<Waker>>,
    notifiable: usize,
}

impl Inner {
     fn new() -> Self {
        Self {
            entries: SlotMap::new(),
            notifiable: 0,
        }
    }

    /// Removes the waker of a cancelled operation.
    ///
    /// Returns `true` if another blocked operation from the set was notified.
    fn cancel(&mut self, key: DefaultKey) -> bool {
        match self.entries.remove(key) {
            Some(_) => self.notifiable -= 1,
            None => {
                // The operation was cancelled and notified so notify another operation instead.
                for (_ ,z) in self.entries.iter_mut() {
                    // If there is no waker in this entry, that means it was already woken.
                    if let Some(w) = z.take() {
                        w.wake();
                        self.notifiable -= 1;
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Notifies a blocked operation if none have been notified already.
    ///
    /// Returns `true` if an operation was notified.
    fn notify_any(&mut self) -> bool {
        self.notify(Notify::Any)
    }

    /// Notifies one additional blocked operation.
    ///
    /// Returns `true` if an operation was notified.
    fn notify_one(&mut self) -> bool {
        self.notify(Notify::One)
    }

    /// Notifies blocked operations, either one or all of them.
    ///
    /// Returns `true` if at least one operation was notified.
    fn notify(&mut self, n: Notify) -> bool {
        let mut notified = false;

        for (_, z) in self.entries.iter_mut() {
            // If there is no waker in this entry, that means it was already woken.
            if let Some(w) = z.take() {
                w.wake();
                self.notifiable -= 1;
                notified = true;

                if n == Notify::One {
                    break;
                }
            }

            if n == Notify::Any {
                break;
            }
        }

        notified
    }

    fn insert(&mut self, cx: &Context<'_>) -> DefaultKey {
        let w = cx.waker().clone();
        let key = self.entries.insert(Some(w));
        self.notifiable += 1;
        key
    }

    /// Removes the waker of an operation.
    fn remove(&mut self, key: DefaultKey) {
        if self.entries.remove(key).is_some() {
            self.notifiable -= 1;
        }
    }
}
