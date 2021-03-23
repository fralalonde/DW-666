use core::array::TryFromSliceError;

use nb;
use usb_device::UsbError;

pub use message::{Message, note_on};
pub use note::Note;
pub use packet::{CableNumber, CodeIndexNumber, Packet};
pub use serial::{SerialIn, SerialOut};
pub use status::Status;
pub use u14::U14;
pub use u4::U4;
pub use u7::U7;
pub use usb::{MidiClass, usb_device, UsbMidi};

mod u4;
mod u7;
mod u14;
mod status;
mod note;
mod message;
mod packet;
mod serial;
mod usb;


pub type Channel = U4;
pub type Velocity = U7;
pub type Control = U7;
pub type Pressure = U7;
pub type Program = U7;
pub type Bend = U14;

pub trait Receive {
    fn receive(&mut self) -> Result<Option<Packet>, MidiError>;
}

pub trait Transmit {
    /// Send a single packet
    fn transmit(&mut self, event: Packet) -> Result<(), MidiError>;

    /// Sending buffered Sysex does not require "packetizing" on serial
    fn transmit_sysex(&mut self, buffer: &[u8]) -> Result<(), MidiError>;
}

#[derive(Debug)]
#[repr(u8)]
pub enum MidiError {
    Unimplemented,
    SysexInterrupted,
    NotAMidiStatus(u8),
    UnparseablePacket(Packet),
    NoModeForParameter,
    SysexOutOfBounds,
    InvalidCodeIndexNumber,
    InvalidCableNumber,
    InvalidChannel,
    InvalidNote,
    InvalidVelocity,
    InvalidU4,
    InvalidU7,
    InvalidU14,
    SerialError,
    ParseCritical,
    TryFromSliceError,
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

impl From<TryFromSliceError> for MidiError {
    fn from(_: TryFromSliceError) -> Self {
        MidiError::TryFromSliceError
    }
}

/// Just strip higher bits (meh)
pub trait Cull<T>: Sized {
    fn cull(_: T) -> Self;
}

/// Saturate to T::MAX
pub trait Saturate<T>: Sized {
    fn saturate(_: T) -> Self;
}
