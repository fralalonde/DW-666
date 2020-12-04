use crate::midi::u4::{InvalidU4};
use nb;
use usb_device::UsbError;
use defmt::Format;
use crate::midi::event::Packet;

pub mod message;
pub mod notes;
// pub mod parser;
// pub mod writer;
pub mod serial;
pub mod u4;
pub mod u7;
pub mod usb;
pub mod event;

// pub type Status = U4;
// pub type MidiChannel = U4;
// pub type MidiControl = U7;

pub trait Receive {
    fn receive(&mut self) -> Result<Option<Packet>, MidiError>;
}

pub trait Send {
    fn send(&mut self, event: Packet) -> Result<(), MidiError>;
}

#[derive(Debug, Format)]
pub enum MidiError {
    PayloadOverflow,
    SysexInterrupted,
    NotAMidiStatus,
    NotAChanelCommand,
    NotASystemCommand,
    UnhandledDecode,
    SysexOutofBounds,
    InvalidU4,
    SerialError,
    UsbError
}

impl From<UsbError> for MidiError {
    fn from(_err: UsbError) -> Self {
        MidiError::UsbError
    }
}

impl From<InvalidU4> for MidiError {
    fn from(_: InvalidU4) -> Self {
        MidiError::InvalidU4
    }
}

impl <E> From<nb::Error<E>> for MidiError {
    fn from(_: nb::Error<E>) -> Self {
        MidiError::SerialError
    }
}



/// Like from, but will conceptually overflow if the value is too big
/// this is useful from going from higher ranges to lower ranges
pub trait FromOverFlow<T>: Sized {
    fn from_overflow(_: T) -> Self;
}

/// Like from, but will clamp the value to a maximum value
pub trait FromClamped<T>: Sized {
    fn from_clamped(_: T) -> Self;
}
