//! Thread-safe reference-counting pointers.
//!
//! See the [`Arc<T>`][Arc] documentation for more details.
//!
#![feature(layout_for_ptr)]
#![feature(coerce_unsized)]
#![feature(dispatch_from_dyn)]
#![feature(receiver_trait)]
#![feature(unsize)]
#![feature(box_syntax)]
#![feature(allocator_api)]
#![feature(ptr_internals)]
#![feature(set_ptr_value)]
#![feature(slice_ptr_get)]
#![feature(specialization)]
#![feature(trusted_len)]
#![feature(core_intrinsics)]
#![feature(alloc_layout_extra)]
#![feature(const_fn_trait_bound)]

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc as core_alloc;

#[cfg(feature = "alloc")]
pub mod alloc;

pub mod array_queue;

pub(crate) mod relax;
pub mod spin;
