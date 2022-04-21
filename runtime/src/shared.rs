use crate::spin::{SpinMutex};

pub struct Shared<T> {
    item: SpinMutex<Option<T>>,
}

// impl<'a, T> Shared<T> {
//     pub fn leak(&self) -> &'a mut Option<T> {
//         SpinMutexGuard::leak(self.item.lock())
//     }
// }

impl<T> Shared<T> {
    pub const fn uninit() -> Self {
        Shared {
            item: SpinMutex::new(None)
        }
    }

    pub fn init_with(&self, new: T) {
        *self.item.lock() = Some(new)
    }

    pub fn lock_then<F: FnOnce(&mut T) -> R, R>(&self, f: F) -> R {
        unsafe {
            f(self.item.lock().as_mut().unwrap_unchecked())
        }
    }
}
