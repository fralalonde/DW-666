use core::array::TryFromSliceError;

mod u4;
mod u6;
mod u7;
mod u14;
mod status;
mod note;
mod message;
mod packet;
mod serial;
mod usb;
mod sysex;
mod route;
mod filter;

use nb;
use usb_device::UsbError;

pub use message::{Message, note_on, program_change, note_off};
pub use note::Note;
pub use packet::{CableNumber, CodeIndexNumber, Packet};
pub use status::Status;
pub use u14::U14;
pub use u4::U4;
pub use u6::U6;
pub use u7::{U7};

pub use serial::{SerialMidi};
pub use usb::{MidiClass, usb_device, UsbMidi};
pub use sysex::{Matcher, Token, Tag, Sysex};
pub use route::{RouteId, Router, RouteBinding, RouteContext, Route, Service, RouterEvent, Handler};
pub use filter::{capture_sysex, print_tag, event_print, Filter};
use num::PrimInt;

#[derive(Clone, Copy, Debug)]
/// MIDI channel, stored as 0-15
pub struct Channel(u8);

/// "Natural" channel builder, takes integers 1-16 as input, wraparound
pub fn channel(ch: impl Into<u8>) -> Channel {
    let mut ch = ch.into() % 16;
    Channel(ch - 1)
}

pub type Velocity = U7;
pub type Control = U7;
pub type Pressure = U7;
pub type Program = U7;
pub type Bend = U14;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Interface {
    USB,
    Serial(u8),
    Virtual(u16),
    // TODO virtual interfaces ?
}

pub trait Receive {
    fn receive(&mut self) -> Result<Option<Packet>, MidiError>;
}

pub trait Transmit {
    /// Send a single packet
    fn transmit(&mut self, event: Packet) -> Result<(), MidiError>;
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
    InvalidProgram,
    InvalidNote,
    InvalidVelocity,
    InvalidU4,
    InvalidU7,
    InvalidU6,
    InvalidU14,
    SerialError,
    ParseCritical,
    TryFromSliceError,
    UsbError,
    BufferFull,
    SysexBufferFull,
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
