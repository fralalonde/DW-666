use nb;
use usb_device::UsbError;

mod u4;
mod u7;
mod u14;
mod status;
mod note;
mod message;
mod packet;
mod serial;
mod usb;

pub use u4::U4;
pub use u7::U7;
pub use u14::U14;

pub use message::{Message, note_on};
pub use packet::{Packet, CodeIndexNumber, CableNumber};
pub use note::Note;
pub use serial::{SerialIn, SerialOut};
pub use usb::{UsbMidi, MidiClass, usb_device};
pub use status::{Status};
use core::array::TryFromSliceError;

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
    fn transmit(&mut self, event: Packet) -> Result<(), MidiError>;
}

#[derive(Debug)]
#[repr(u8)]
pub enum MidiError {
    Unimplemented,
    SysexInterrupted,
    NotAMidiStatus(u8),
    UnparseablePacket(Packet),
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
