#![no_std]

#[cfg(feature = "rtt")]
#[macro_use]
extern crate rtt_target;

use core::array::TryFromSliceError;
use core::iter::FromIterator;
use core::ops::{Deref, DerefMut};

use heapless::Vec;
use nb;
use usb_device::UsbError;

pub use message::{Message, note_off, note_on, program_change};
pub use note::Note;
pub use packet::{CableNumber, CodeIndexNumber, Packet};

pub use status::Status;
pub use u14::U14;
pub use u4::U4;
pub use u6::U6;
pub use u7::U7;
pub use parser::{PacketParser};
pub use status::is_channel_status;
pub use status::is_non_status;


mod u4;
mod u6;
mod u7;
mod u14;
mod status;
mod note;
mod message;
mod packet;
mod parser;

// macro_rules! dbgprint {
//     ($($arg:tt)*) => {{}};
// }

#[derive(Clone, Copy, Debug)]
/// MIDI channel, stored as 0-15
pub struct Channel(pub u8);

/// "Natural" channel builder, takes integers 1-16 as input, wraparound
/// FIXME rollover fails in checked builds!
pub fn channel(ch: impl Into<u8>) -> Channel {
    let ch = (ch.into() - 1).min(15);
    Channel(ch)
}

pub type Velocity = U7;
pub type Control = U7;
pub type Pressure = U7;
pub type Program = U7;
pub type Bend = U14;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Interface {
    USB(u8),
    Serial(u8),
}

#[derive(Copy, Clone, Debug)]
pub enum Binding {
    Src(Interface),
    Dst(Interface),
}

#[derive(Copy, Clone, Debug)]
pub struct Endpoint {
    pub interface: Interface,
    pub channel: Channel,
}

impl From<(Interface, Channel)> for Endpoint {
    fn from(pa: (Interface, Channel)) -> Self {
        Endpoint { interface: pa.0, channel: pa.1 }
    }
}

pub trait Receive {
    fn receive(&mut self) -> Result<Option<Packet>, MidiError>;
}

#[derive(Default, Debug, Clone)]
pub struct PacketList(Vec<Packet, 16>);

impl Deref for PacketList {
    type Target = Vec<Packet, 16>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PacketList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromIterator<Packet> for PacketList {
    fn from_iter<T: IntoIterator<Item=Packet>>(iter: T) -> Self {
        let mut list = Vec::new();
        for p in iter {
            if let Err(_) = list.push(p) {
                break;
            }
        }
        PacketList(list)
    }
}

impl PacketList {
    pub fn single(packet: Packet) -> Self {
        let mut list = Vec::new();
        let _ = list.push(packet);
        PacketList(list)
    }
}

pub trait Transmit {
    /// Send a single packet
    fn transmit(&mut self, event: PacketList) -> Result<(), MidiError>;
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
    /// RTIC queue full?
    UnsentPacket,
    UnsentString,
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

/// RTIC spawn error
impl From<TryFromSliceError> for MidiError {
    fn from(_: TryFromSliceError) -> Self {
        MidiError::TryFromSliceError
    }
}

/// RTIC spawn error
impl From<(Binding, PacketList)> for MidiError {
    fn from(_: (Binding, PacketList)) -> Self {
        MidiError::UnsentPacket
    }
}

/// RTIC spawn error
impl From<(Interface, PacketList)> for MidiError {
    fn from(_: (Interface, PacketList)) -> Self {
        MidiError::UnsentPacket
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
