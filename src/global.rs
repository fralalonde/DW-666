#[global_allocator]
static ALLOCATOR: RusPiRoAllocator = RusPiRoAllocator;

use core::alloc::{GlobalAlloc, Layout};

mod memory {

    //! # Lock Free Memory Management

    use core::sync::atomic::{AtomicUsize, Ordering};

    /// The magic identifier for a managed memory block
    const MM_MAGIC: u32 = 0xDEAD_BEEF;

    /// Memory allocations happens in predefined chunk sizes. This might lead to memory wast in some cases
    /// but this could help increasing the speed for re-usage of freed memory regions as we know which
    /// bucket to look for when re-using. Memory requirements above 1MB are handled individually w/o any
    /// bucket assignment
    #[repr(u32)]
    #[derive(Copy, Clone, Debug)]
    enum MemBucketSize {
        _16B = 0x00_0010,
        _32B = 0x00_0020,
        _64B = 0x00_0040,
        _128B = 0x00_0080,
        _256B = 0x00_0100,
        _512B = 0x00_0200,
        _1KB = 0x00_0400,
        _2KB = 0x00_0800,
    }

    /// Need to place the enum values also in an array to be able to iterate over them :/
    const BUCKET_SIZES: [MemBucketSize; 8] = [
        MemBucketSize::_16B,
        MemBucketSize::_32B,
        MemBucketSize::_64B,
        MemBucketSize::_128B,
        MemBucketSize::_256B,
        MemBucketSize::_512B,
        MemBucketSize::_1KB,
        MemBucketSize::_2KB,
    ];

    // extern "C" {
    //     /// Linker Symbol which address points to the HEAP START.
    //     /// Access as &cortex_m_rt::heap_start() -> address!
    //     static cortex_m_rt::heap_start(): usize;
    //     // /// Linker Symbol which address points to the HEAP END . On a Raspberry Pi this should be treated with
    //     // /// care as the whole HEAP is shared between the ARM CPU and GPU. Only a mailbox call can provide
    //     // /// the real ARM HEAP size
    //     // static __heap_end: usize;
    // }

    /// Descriptive block of a managed memory reagion. This administrative data is stored along side with
    /// the actual memory allocated. This means the physical memory requirement is always the requested
    /// one + the size of this descriptor
    #[repr(C, packed)]
    #[derive(Copy, Clone, Default, Debug)]
    struct MemoryDescriptor {
        /// The magic of this block
        magic: u32,
        /// The bucket index this memory block is assigned to
        bucket: usize,
        /// The real occupied memory size (descriptor size + payload size)
        size: usize,
        align: usize,
        /// Address of the preceding memory block when this one is ready for re-use
        prev: usize,
        /// Address of the following memory block when this one is ready for re-use
        next: usize,
        /// payload address. In addition the address of the descritor managing this memory need to be
        /// stored relative to the address stored here to ensure we can calculate the descriptor address
        /// back from the payload address in case we were ask to free this location
        payload_addr: usize,
        /// this placeholder ensures that the payload starts earliest after this usize field. If this is
        /// the case this field will contain the address of the descriptor which need to be stored relative
        /// to the payload start address
        _placeholder: usize,
    }

    struct BucketQueue {
        head: AtomicUsize,
        tail: AtomicUsize,
    }

    /// The global pointer to the next free memory location on the HEAP not considering re-usage. If no
    /// re-usable bucket exists, memory will be allocated at this position. It's implemented as
    /// ``usize`` to ensure we can perform immediate atomic math operation (add/sub) on it.
    static HEAP_START: AtomicUsize = AtomicUsize::new(0);

    /// The list of buckets that may contain re-usable memory blocks. The new free memory blocks are added always to the
    /// tail of each list, while the retrival always happens from the head. Like FIFO buffer
    static FREE_BUCKETS: [BucketQueue; BUCKET_SIZES.len() + 1] = [
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },
        BucketQueue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        },

    ];

    /// Allocate an arbitrary size of memory on the HEAP
    /// The alignment is given in Bytes and need to be a power of 2
    pub(crate) fn alloc(req_size: usize, alignment: usize) -> *mut u8 {
        // if the HEAP START is initial (0) set the address from the linker script
        HEAP_START.compare_and_swap(
            0,
            cortex_m_rt::heap_start() as usize,
            Ordering::AcqRel,
        );

        // calculate the required size to be allocated including descriptor size and alignment
        let padding = alignment; //1 << alignment;
        let admin_size = core::mem::size_of::<MemoryDescriptor>() + padding;
        // calculate the physical size in memory that is required to be allocated
        let phys_size = admin_size + req_size;

        // the physical size defines the bucket this allocation will fall into, so get the smallest bucket
        // where this size would fit
        let bucket_idx = BUCKET_SIZES
            .iter()
            .position(|&bucket| phys_size < bucket as usize);

        // if a bucket could be found allocate its size, otherwise allocate the requested size w/o a bucket assignment
        let alloc_size = bucket_idx.map_or(phys_size, |b| BUCKET_SIZES[b] as usize);
        let bucket = bucket_idx.unwrap_or_else(|| BUCKET_SIZES.len());

        // check if we can get the next position to allocate memory from a re-usable bucket.
        // if this is not the case we retrieve this from the end of the current heap. Both is crucial to
        // get right in the concurrent/multicore access scenario
        let descriptor_addr = pop_from_free_bucket(bucket, alloc_size)
            .unwrap_or_else(|| HEAP_START.fetch_add(alloc_size, Ordering::SeqCst));
        //let descriptor_addr = HEAP_START.fetch_add(alloc_size, Ordering::SeqCst);

        assert!(descriptor_addr < 0x3f00_0000);
        // any other concurrent allocation will now see the new HEAP_START, so we can now maintain the
        // descriptor at the given location
        let descriptor = unsafe { &mut *(descriptor_addr as *mut MemoryDescriptor) };

        // now fill the memory descriptor managing this allocation
        descriptor.magic = MM_MAGIC;
        descriptor.bucket = bucket;
        descriptor.size = alloc_size;
        descriptor.align = alignment;
        descriptor.prev = 0;
        descriptor.next = 0;
        descriptor._placeholder = 0;
        descriptor.payload_addr = (descriptor_addr + admin_size) & !(padding - 1);
        assert!(descriptor.payload_addr > descriptor_addr + core::mem::size_of::<MemoryDescriptor>());

        // the usable address is stored in the payload attribute of the descriptor, however,
        // while releasing memory with this address given, we need a way to calculate the MemoryDescriptor location from
        // there. This is done by keeping at least 1 ``usize`` location free in front of the usage
        // memory location and store the descriptor address there
        let descriptor_link_store = descriptor.payload_addr - core::mem::size_of::<usize>();
        unsafe { *(descriptor_link_store as *mut usize) = descriptor_addr };
        // now hand out the actual payload address pointing to the allocated memory with at least the requested size
        descriptor.payload_addr as *mut u8
    }

    /// allocate memory in chunks of pages, where the page size depends on the architecture and is therefore given from the
    /// caller. It always allocates memory that is alligned to the page boundaries and occupies (num * page_size) memory on
    /// the heap
    #[allow(dead_code)]
    pub(crate) fn alloc_page(num: usize, page_size: usize) -> *mut u8 {
        // for the time beeing we will always allocate fresh memory from the heap for this kind of allocation
        // and do never check available free buckets
        // if the HEAP START is initial (0) set the address from the linker script
        HEAP_START.compare_and_swap(
            0,
            cortex_m_rt::heap_start() as usize,
            Ordering::AcqRel,
        );

        // from the current HEAP_START calculate the next start address of a page
        let mut heap_start = HEAP_START.load(Ordering::Acquire);
        let heap_align = (heap_start + page_size - 1) & !(page_size - 1);
        // if the aligned address does not allow enough space for the memory descriptor we need
        // "waste" some memory and go to the next page start address
        if (heap_align - heap_start) < core::mem::size_of::<MemoryDescriptor>() {
            heap_start = heap_align + page_size;
        } else {
            heap_start = heap_align;
        }

        // as we now know where the requested memory will start and end we can update the HEAP_START accordingly to let
        // others know where to request memory from
        HEAP_START.store(heap_start + num * page_size, Ordering::Release); // from her other cores will be able to access
        // this as well

        let alloc_size = num * page_size + core::mem::size_of::<MemoryDescriptor>();
        let descriptor_addr = heap_start - core::mem::size_of::<MemoryDescriptor>();
        // fill the descriptor structure
        let descriptor = unsafe { &mut *(descriptor_addr as *mut MemoryDescriptor) };

        // now fill the memory descriptor managing this allocation
        descriptor.magic = MM_MAGIC;
        descriptor.bucket = BUCKET_SIZES.len();
        descriptor.size = alloc_size;
        descriptor.align = page_size;
        descriptor.prev = 0;
        descriptor.next = 0;
        descriptor._placeholder = 0;
        descriptor.payload_addr = heap_start;
        assert!(descriptor.payload_addr < 0x3f00_0000);

        // the usable address is stored in the payload attribute of the descriptor, however,
        // while releasing memory with this address given, we need a way to calculate the MemoryDescriptor location from
        // there. This is done by keeping at least 1 ``usize`` location free in front of the usage
        // memory location and store the descriptor address there
        let descriptor_link_store = descriptor.payload_addr - core::mem::size_of::<usize>();
        unsafe { *(descriptor_link_store as *mut usize) = descriptor_addr };
        //info!("{:#x?} -> {:#x?}, linkstore: {:#x?}", descriptor_addr, descriptor, descriptor_link_store);
        // now hand out the actual payload address pointing to the allocated memory with at least the requested size
        descriptor.payload_addr as *mut u8
    }

    /// Free the memory occupied by the given payload pointer
    pub(crate) fn free(address: *mut u8) {
        // first get the address of the descriptor for this payload pointer
        let descriptor_link_store = (address as usize) - core::mem::size_of::<usize>();
        let descriptor_addr = unsafe { *(descriptor_link_store as *const usize) };
        let mut descriptor = unsafe { &mut *(descriptor_addr as *mut MemoryDescriptor) };
        assert!(descriptor.magic == MM_MAGIC);
        // clean the magic of this memory block
        descriptor.magic = 0;
        // we now know the data of this memory descriptor, add this one to the corresponding free bucket
        // or just adjust the heap pointer if this is the last memory entry that is about to be freed
        let heap_check = descriptor_addr + descriptor.size;
        // updating the heap pointer is the critical part here for concurrent access. So once this happened
        // this location might be used for allocations. So we shall never ever access parts of this location
        // any more if the swap was successfull
        let prev_heap_start =
            HEAP_START.compare_and_swap(heap_check, descriptor_addr, Ordering::SeqCst);
        if prev_heap_start == heap_check {
            // we are done
            return;
        }
        // it's not a memory region at the end of the heap, so put it into the corresponding bucket
        push_to_free_bucket(descriptor);
    }

    #[inline]
    fn push_to_free_bucket(descriptor: &mut MemoryDescriptor) {
        // setting this bucket as the new last free entry is a crucial operation in concurrent access.
        // as soon as this happened any other access sees the new entry
        // as we need to set the previous bucket in the new one while ensuring concurrent access is not
        // re-using this block while doing so we need to do this in steps until we set the new free bucket
        let descriptor_addr = descriptor as *mut MemoryDescriptor as usize;
        loop {
            // 1. load the previous free bucket
            let prev_free_bucket = FREE_BUCKETS[descriptor.bucket].tail.load(Ordering::Acquire);
            // 2. store this address in the new free bucket
            descriptor.prev = prev_free_bucket;
            descriptor.next = 0;
            // 3. swap the old and the new free bucket if the old free bucket is still the same
            let prev_free_bucket_check = FREE_BUCKETS[descriptor.bucket].tail.compare_and_swap(
                prev_free_bucket,
                descriptor_addr,
                Ordering::SeqCst,
            );
            // 4. if the free bucket was different re-try as it has been occupied in the meanwhile
            if prev_free_bucket == prev_free_bucket_check {
                // 5. if we have successfully pushed this to the tail, update the next pointer in the previous
                // descriptor to make the chain complete
                if prev_free_bucket != 0 {
                    let prev_descriptor = unsafe { &mut *(prev_free_bucket as *mut MemoryDescriptor) };
                    prev_descriptor.next = descriptor_addr;
                } else {
                    // 6. if the previous free bucket was not set the head is also not set, so update the head to the new
                    // free bucket as well
                    FREE_BUCKETS[descriptor.bucket]
                        .head
                        .store(descriptor_addr, Ordering::SeqCst);
                }
                return;
            }
        }
    }

    // get the next free re-usable bucket to allocate the memory from
    #[inline]
    fn pop_from_free_bucket(bucket: usize, _alloc_size: usize) -> Option<usize> {
        assert!(bucket < FREE_BUCKETS.len());
        // TODO: dynamically sized buckets need special treatment to see if the requested size will fit into one. This
        // is not yet properly tested, so for the time beeing any dynamically sized freed bucket will never be re-used
        // but memory will always be requested from the HEAP end.
        if bucket == BUCKET_SIZES.len() {
            // no reusable memory block found --> trigger allocation from fresh heap ...
            return None;
        } else {
            // first check if we have re-usable memory available in the corresponding bucket
            let reusable_bucket = FREE_BUCKETS[bucket].head.load(Ordering::Acquire);
            // if this is available use it as the free slot, so replace this free bucket with it's next
            // one. This is crucial in cuncurrent access so do this only if this still is the same free bucket
            if reusable_bucket != 0 {
                let descriptor = unsafe { &*(reusable_bucket as *const MemoryDescriptor) };
                let reusable_bucket_check = FREE_BUCKETS[bucket].head.compare_and_swap(
                    reusable_bucket,
                    descriptor.next,
                    Ordering::Release,
                );
                if reusable_bucket_check == reusable_bucket {
                    if descriptor.next != 0 {
                        // if we had a next block update it's previous one
                        let next_descriptor =
                            unsafe { &mut *(descriptor.next as *mut MemoryDescriptor) };
                        next_descriptor.prev = 0;
                    } else {
                        // clear the tail as this was the last entry in the list
                        FREE_BUCKETS[bucket].tail.store(0, Ordering::SeqCst);
                    }
                    // use the reusable bucket as new memory block
                    return Some(reusable_bucket);
                } else {
                    // the re-usable bucket has been occupied since the last read, so continue with
                    // allocating from the heap
                    return None;
                }
            }
        }

        None
    }

}

struct RusPiRoAllocator;

unsafe impl GlobalAlloc for RusPiRoAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        memory::alloc(layout.size(), layout.align())
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        memory::free(ptr)
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = memory::alloc(layout.size(), layout.align());
        memset(ptr, 0x0, layout.size());
        ptr
    }
}

#[cfg(not(any(test, doctest)))]
#[alloc_error_handler]
#[allow(clippy::empty_loop)]
fn alloc_error_handler(_: Layout) -> ! {
    // TODO: how to handle memory allocation errors?
    loop {}
}

extern "C" {
    // reference to the compiler built-in function
    fn memset(ptr: *mut u8, value: i32, size: usize) -> *mut u8;
}


// //! Bump pointer allocator for *single* core systems
// //! Taken from the embedded Rust book. Whatever.
//
// use core::alloc::{GlobalAlloc, Layout};
// use core::cell::UnsafeCell;
// use core::ptr::NonNull;
// use core::{mem, ptr};
// use cortex_m::{asm, interrupt};
//
// // Global memory allocator
// // NOTE ensure that the memory region `[0x2000_0100, 0x2000_0200]` is not used anywhere else
// const RAM_START: usize = 0x2000_0000;
// const HEAP_START: usize = RAM_START;
// const HEAP_END: usize = RAM_START + (10 * 1024); // 8k Heap Size
//
// // #[global_allocator]
// // static HEAP: BumpPointerAlloc = BumpPointerAlloc {
// //     head: UnsafeCell::new(HEAP_START),
// //     end: HEAP_END, // ens of 48k
// // };
//
// /// A wrapper around spin::Mutex to permit trait implementations.
// pub struct Locked<A> {
//     inner: spin::Mutex<A>,
// }
//
// impl<A> Locked<A> {
//     pub const fn new(inner: A) -> Self {
//         Locked {
//             inner: spin::Mutex::new(inner),
//         }
//     }
//
//     pub fn lock(&self) -> spin::MutexGuard<A> {
//         self.inner.lock()
//     }
// }
//
// #[global_allocator]
// static ALLOCATOR: Locked<FixedSizeBlockAllocator> = Locked::new(FixedSizeBlockAllocator::new());
//
// #[alloc_error_handler]
// fn on_oom(_layout: Layout) -> ! {
//     asm::bkpt();
//     loop {}
// }
//
// struct ListNode {
//     next: Option<&'static mut ListNode>,
// }
//
// /// The block sizes to use.
// ///
// /// The sizes must each be power of 2 because they are also used as
// /// the block alignment (alignments must be always powers of 2).
// const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];
//
// /// Choose an appropriate block size for the given layout.
// ///
// /// Returns an index into the `BLOCK_SIZES` array.
// fn list_index(layout: &Layout) -> Option<usize> {
//     let required_block_size = layout.size().max(layout.align());
//     BLOCK_SIZES.iter().position(|&s| s >= required_block_size)
// }
//
// pub struct FixedSizeBlockAllocator {
//     list_heads: [Option<&'static mut ListNode>; BLOCK_SIZES.len()],
//     // fallback_allocator: linked_list_allocator::Heap,
// }
//
// impl FixedSizeBlockAllocator {
//     /// Creates an empty FixedSizeBlockAllocator.
//     pub const fn new() -> Self {
//         FixedSizeBlockAllocator {
//             list_heads: [None; BLOCK_SIZES.len()],
//             // fallback_allocator: linked_list_allocator::Heap::empty(),
//         }
//     }
//
//     /// Initialize the allocator with the given heap bounds.
//     ///
//     /// This function is unsafe because the caller must guarantee that the given
//     /// heap bounds are valid and that the heap is unused. This method must be
//     /// called only once.
//     pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
//         // self.fallback_allocator.init(heap_start, heap_size);
//     }
//
//     /// Allocates using the fallback allocator.
//     // fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
//     //     match self.fallback_allocator.allocate_first_fit(layout) {
//     //         Ok(ptr) => ptr.as_ptr(),
//     //         Err(_) => ptr::null_mut(),
//     //     }
//     // }
//
//     unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
//         match list_index(&layout) {
//             Some(index) => {
//                 match self.list_heads[index].take() {
//                     Some(node) => {
//                         self.list_heads[index] = node.next.take();
//                         node as *mut ListNode as *mut u8
//                     }
//                     _ => panic!("alloc failed")
//                     // None => {
//                     //     // no block exists in list => allocate new block
//                     //     let block_size = BLOCK_SIZES[index];
//                     //     // only works if all block sizes are a power of 2
//                     //     let block_align = block_size;
//                     //     let layout = Layout::from_size_align(block_size, block_align).unwrap();
//                     //     self.fallback_alloc(layout)
//                     // }
//                 }
//             }
//             // None => self.fallback_alloc(layout),
//             _ => panic!("alloc failed")
//         }
//     }
//
//     unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
//         match list_index(&layout) {
//             Some(index) => {
//                 let new_node = ListNode {
//                     next: self.list_heads[index].take(),
//                 };
//                 // verify that block has size and alignment required for storing node
//                 assert!(mem::size_of::<ListNode>() <= BLOCK_SIZES[index]);
//                 assert!(mem::align_of::<ListNode>() <= BLOCK_SIZES[index]);
//                 let new_node_ptr = ptr as *mut ListNode;
//                 new_node_ptr.write(new_node);
//                 self.list_heads[index] = Some(&mut *new_node_ptr);
//             }
//             // None => {
//             //     let ptr = NonNull::new(ptr).unwrap();
//             //     self.fallback_allocator.deallocate(ptr, layout);
//             // }
//             _ => panic!("dealloc failed")
//         }
//     }
// }
//
// unsafe impl GlobalAlloc for Locked<FixedSizeBlockAllocator> {
//     unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
//         self.lock().alloc(layout)
//     }
//
//     unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
//         self.lock().dealloc(ptr, layout);
//     }
// }
//
// // pub struct BumpPointerAlloc {
// //     pub head: UnsafeCell<usize>,
// //     pub end: usize,
// // }
// //
// // unsafe impl Sync for BumpPointerAlloc {}
// //
// // unsafe impl GlobalAlloc for BumpPointerAlloc {
// //     unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
// //         // `interrupt::free` is a critical section that makes our allocator safe to use from within interrupts
// //         interrupt::free(|_| {
// //             let head = self.head.get();
// //             let size = layout.size();
// //             let align = layout.align();
// //             let align_mask = !(align - 1);
// //
// //             // move start up to the next alignment boundary
// //             let start = (*head + align - 1) & align_mask;
// //
// //             if start + size >= self.end {
// //                 // a null pointer signal an Out Of Memory condition
// //                 ptr::null_mut()
// //             } else {
// //                 *head = start + size;
// //                 start as *mut u8
// //             }
// //         })
// //     }
// //
// //     unsafe fn dealloc(&self, _: *mut u8, _: Layout) {
// //         // this allocator never deallocates memory
// //     }
// // }
