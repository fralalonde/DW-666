//! Bump pointer allocator for *single* core systems
//! Taken from the embedded Rust book. Whatever.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use core::{mem, ptr};
use cortex_m::{asm, interrupt};

// Global memory allocator
// NOTE ensure that the memory region `[0x2000_0100, 0x2000_0200]` is not used anywhere else
const RAM_START: usize = 0x2000_0000;
const HEAP_START: usize = RAM_START;
const HEAP_END: usize = RAM_START + (10 * 1024); // 8k Heap Size

// #[global_allocator]
// static HEAP: BumpPointerAlloc = BumpPointerAlloc {
//     head: UnsafeCell::new(HEAP_START),
//     end: HEAP_END, // ens of 48k
// };

/// A wrapper around spin::Mutex to permit trait implementations.
pub struct Locked<A> {
    inner: spin::Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: spin::Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.inner.lock()
    }
}

#[global_allocator]
static ALLOCATOR: Locked<FixedSizeBlockAllocator> = Locked::new(FixedSizeBlockAllocator::new());

#[alloc_error_handler]
fn on_oom(_layout: Layout) -> ! {
    asm::bkpt();
    loop {}
}

struct ListNode {
    next: Option<&'static mut ListNode>,
}

/// The block sizes to use.
///
/// The sizes must each be power of 2 because they are also used as
/// the block alignment (alignments must be always powers of 2).
const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

/// Choose an appropriate block size for the given layout.
///
/// Returns an index into the `BLOCK_SIZES` array.
fn list_index(layout: &Layout) -> Option<usize> {
    let required_block_size = layout.size().max(layout.align());
    BLOCK_SIZES.iter().position(|&s| s >= required_block_size)
}

pub struct FixedSizeBlockAllocator {
    list_heads: [Option<&'static mut ListNode>; BLOCK_SIZES.len()],
    // fallback_allocator: linked_list_allocator::Heap,
}

impl FixedSizeBlockAllocator {
    /// Creates an empty FixedSizeBlockAllocator.
    pub const fn new() -> Self {
        FixedSizeBlockAllocator {
            list_heads: [None; BLOCK_SIZES.len()],
            // fallback_allocator: linked_list_allocator::Heap::empty(),
        }
    }

    /// Initialize the allocator with the given heap bounds.
    ///
    /// This function is unsafe because the caller must guarantee that the given
    /// heap bounds are valid and that the heap is unused. This method must be
    /// called only once.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        // self.fallback_allocator.init(heap_start, heap_size);
    }

    /// Allocates using the fallback allocator.
    // fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
    //     match self.fallback_allocator.allocate_first_fit(layout) {
    //         Ok(ptr) => ptr.as_ptr(),
    //         Err(_) => ptr::null_mut(),
    //     }
    // }

    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match list_index(&layout) {
            Some(index) => {
                match self.list_heads[index].take() {
                    Some(node) => {
                        self.list_heads[index] = node.next.take();
                        node as *mut ListNode as *mut u8
                    }
                    _ => panic!("alloc failed")
                    // None => {
                    //     // no block exists in list => allocate new block
                    //     let block_size = BLOCK_SIZES[index];
                    //     // only works if all block sizes are a power of 2
                    //     let block_align = block_size;
                    //     let layout = Layout::from_size_align(block_size, block_align).unwrap();
                    //     self.fallback_alloc(layout)
                    // }
                }
            }
            // None => self.fallback_alloc(layout),
            _ => panic!("alloc failed")
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match list_index(&layout) {
            Some(index) => {
                let new_node = ListNode {
                    next: self.list_heads[index].take(),
                };
                // verify that block has size and alignment required for storing node
                assert!(mem::size_of::<ListNode>() <= BLOCK_SIZES[index]);
                assert!(mem::align_of::<ListNode>() <= BLOCK_SIZES[index]);
                let new_node_ptr = ptr as *mut ListNode;
                new_node_ptr.write(new_node);
                self.list_heads[index] = Some(&mut *new_node_ptr);
            }
            // None => {
            //     let ptr = NonNull::new(ptr).unwrap();
            //     self.fallback_allocator.deallocate(ptr, layout);
            // }
            _ => panic!("dealloc failed")
        }
    }
}

unsafe impl GlobalAlloc for Locked<FixedSizeBlockAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.lock().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.lock().dealloc(ptr, layout);
    }
}

// pub struct BumpPointerAlloc {
//     pub head: UnsafeCell<usize>,
//     pub end: usize,
// }
//
// unsafe impl Sync for BumpPointerAlloc {}
//
// unsafe impl GlobalAlloc for BumpPointerAlloc {
//     unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
//         // `interrupt::free` is a critical section that makes our allocator safe to use from within interrupts
//         interrupt::free(|_| {
//             let head = self.head.get();
//             let size = layout.size();
//             let align = layout.align();
//             let align_mask = !(align - 1);
//
//             // move start up to the next alignment boundary
//             let start = (*head + align - 1) & align_mask;
//
//             if start + size >= self.end {
//                 // a null pointer signal an Out Of Memory condition
//                 ptr::null_mut()
//             } else {
//                 *head = start + size;
//                 start as *mut u8
//             }
//         })
//     }
//
//     unsafe fn dealloc(&self, _: *mut u8, _: Layout) {
//         // this allocator never deallocates memory
//     }
// }
