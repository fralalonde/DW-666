//! USB Host driver implementation for SAMD* series chips.
//! Refer to Atmel SMART SAM SD21 Datasheet for detailed explanation of registers and shit
#![no_std]

#[macro_use]
extern crate async_trait;

#[macro_use]
extern crate runtime;

extern crate alloc;

pub mod usb_host;

mod host;
mod pipe;
mod error;

use crate::usb_host::DeviceDescriptor;

pub use host::{SAMDHost, Pins, HostEvent};
use crate::usb_host::address::Address;


