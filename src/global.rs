//! Bump pointer allocator for *single* core systems
//! Taken from the embedded Rust book. Whatever.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr;
use cortex_m::{asm, interrupt};

// Global memory allocator
// NOTE ensure that the memory region `[0x2000_0100, 0x2000_0200]` is not used anywhere else
const RAM_START: usize = 0x2000_0000;
const HEAP_START: usize = RAM_START;
const HEAP_END: usize = RAM_START + (10 * 1024); // 8k Heap Size

#[global_allocator]
static HEAP: BumpPointerAlloc = BumpPointerAlloc {
    head: UnsafeCell::new(HEAP_START),
    end: HEAP_END, // ens of 48k
};

#[alloc_error_handler]
fn on_oom(_layout: Layout) -> ! {
    asm::bkpt();
    loop {}
}

pub struct BumpPointerAlloc {
    pub head: UnsafeCell<usize>,
    pub end: usize,
}

unsafe impl Sync for BumpPointerAlloc {}

unsafe impl GlobalAlloc for BumpPointerAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `interrupt::free` is a critical section that makes our allocator safe to use from within interrupts
        interrupt::free(|_| {
            let head = self.head.get();
            let size = layout.size();
            let align = layout.align();
            let align_mask = !(align - 1);

            // move start up to the next alignment boundary
            let start = (*head + align - 1) & align_mask;

            if start + size >= self.end {
                // a null pointer signal an Out Of Memory condition
                ptr::null_mut()
            } else {
                *head = start + size;
                start as *mut u8
            }
        })
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {
        // this allocator never deallocates memory
    }
}
