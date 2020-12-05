use nb;
use usb_device::UsbError;
use defmt::Format;
use crate::midi::packet::MidiPacket;

pub mod u4;
pub mod u7;
pub mod u14;
pub mod status;
pub mod notes;
pub mod message;
pub mod packet;
pub mod serial;
pub mod usb;

pub trait Receive {
    fn receive(&mut self) -> Result<Option<MidiPacket>, MidiError>;
}

pub trait Transmit {
    fn transmit(&mut self, event: MidiPacket) -> Result<(), MidiError>;
}

#[derive(Debug, Format)]
pub enum MidiError {
    PayloadOverflow,
    SysexInterrupted,
    NotAMidiStatus,
    NotAChanelCommand,
    NotASystemCommand,
    UnhandledDecode,
    SysexOutOfBounds,
    InvalidU4,
    InvalidU7,
    InvalidU14,
    SerialError,
    UsbError,
}

impl From<UsbError> for MidiError {
    fn from(_err: UsbError) -> Self {
        MidiError::UsbError
    }
}

impl<E> From<nb::Error<E>> for MidiError {
    fn from(_: nb::Error<E>) -> Self {
        MidiError::SerialError
    }
}

/// Just strip higher bits (meh)
pub trait Cull<T>: Sized {
    fn cull(_: T) -> Self;
}

/// Saturate to MAX
pub trait Saturate<T>: Sized {
    fn saturate(_: T) -> Self;
}
