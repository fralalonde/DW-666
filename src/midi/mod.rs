use crate::midi::u4::U4;
use crate::midi::u7::U7;

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

pub enum MidiError {
    PayloadOverflow,
    SysexInterrupted,
    NotAMidiStatus,
    NotAChanelCommand,
    NotASystemCommand,
    UnhandledDecode,
    SysexOutofBounds,
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
