//! This crate defines a set of traits for use on the host side of the USB.
//! The `USBHost` defines the Host Controller Interface that can be used by the `Driver` interface.
//! The `Driver` interface defines the set of functions necessary to use devices plugged into the host.

pub mod descriptor;
pub mod control;
pub mod parser;
pub mod device;
pub mod address;
pub mod endpoint;

use core::mem;
pub use descriptor::*;
pub use control::*;
pub use endpoint::*;

use crate::usb_host::device::Device;
use crate::usb_host::parser::DescriptorParser;

/// Errors that can be generated when attempting to do a USB transfer.
#[derive(Debug, defmt::Format)]
pub enum TransferError {
    /// An error that may be retried.
    Retry(&'static str),

    /// A permanent error.
    Permanent(&'static str),
}

pub fn to_slice_mut<T>(v: &mut T) -> &mut [u8] {
    let ptr = v as *mut T as *mut u8;
    unsafe { core::slice::from_raw_parts_mut(ptr, mem::size_of::<T>()) }
}

pub trait USBHost: Send + Sync {
    fn get_host_id(&self) -> u8;

    /// Issue a control transfer with an optional data stage to `ep`
    /// The data stage direction is determined by the direction of `bm_request_type`
    ///
    /// On success, the amount of data transferred into `buf` is returned.
    fn control_transfer(&mut self, ep: &mut Endpoint, bm_request_type: RequestType, b_request: RequestCode, w_value: WValue, w_index: u16, buf: Option<&mut [u8]>) -> Result<usize, TransferError>;

    /// Issue a transfer from `ep` to the host
    /// On success, the amount of data transferred into `buf` is returned
    fn in_transfer(&mut self, ep: &mut Endpoint, buf: &mut [u8]) -> Result<usize, TransferError>;

    /// Issue a transfer from the host to `ep`
    /// All buffer is sent or transfer fails
    fn out_transfer(&mut self, ep: &mut Endpoint, buf: &[u8]) -> Result<usize, TransferError>;

}

/// The type of transfer to use when talking to USB devices.
///
/// cf §9.6.6 of USB 2.0
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TransferType {
    /// High priority, low-level (Configuration, etc.)
    /// Some devices use control transfers for application data
    Control = 0,
    /// Constant throughput, reserved but possibly lossy (Video or audio stream, etc.)
    Isochronous = 1,
    /// Low priority, high throughput (Mass storage, network, etc.)
    Bulk = 2,
    /// High priority, low throughput (Mouse, Keyboard, etc.)
    Interrupt = 3,
}

impl From<u8> for TransferType {
    fn from(byte: u8) -> Self {
        match byte & 0b11 {
            0 => TransferType::Control,
            1 => TransferType::Isochronous,
            2 => TransferType::Bulk,
            3 => TransferType::Interrupt,
            _ => unreachable!()
        }
    }
}

/// Types of errors that can be returned from a `Driver`
#[derive(Copy, Clone, Debug, defmt::Format)]
pub enum DriverError {
    /// An error that may be retried
    Retry(u8, &'static str),

    /// A permanent error.
    Permanent(u8, &'static str),
}

/// Trait for drivers on the USB host.
pub trait Driver: core::fmt::Debug + Send {

    /// Add `device` with address `address` to the driver's registry,
    /// if necessary.
    fn connected(&mut self, host: &mut dyn USBHost, device: &mut Device, desc: &DeviceDescriptor, cfg_desc: &mut DescriptorParser) -> Result<bool, TransferError>;

    /// Remove the device at address `address` from the driver's
    /// registry, if necessary.
    fn disconnected(&mut self, host: &mut dyn USBHost, device: &mut Device);

    /// Called regularly by the USB host to allow the driver to do any
    /// work necessary on its registered devices
    ///
    /// `millis` is the current time, in milliseconds from some arbitrary starting point.
    /// It should be expected that after a long enough run-time, this value will wrap
    ///
    /// `usbhost` may be used for communication with the USB when
    fn tick(&mut self,  usbhost: &mut dyn USBHost) -> Result<(), DriverError>;
}
