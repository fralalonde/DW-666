use crate::midi::u7::U7;
use crate::midi::{MidiError};
use core::convert::TryFrom;
use self::ChannelCommand::*;
use MidiStatus::{ChannelStatus, SystemStatus};
use SystemCommand::*;
use crate::midi::u4::U4;

use num_enum::TryFromPrimitive;

pub type Channel = U4;
pub type Velocity = U7;
pub type Control = U7;

pub enum MidiStatus {
    ChannelStatus(ChannelCommand, Channel),
    SystemStatus(SystemCommand),
}

impl MidiStatus {
    /// Returns expected size in bytes of associated MIDI message
    /// Including the status byte itself
    /// Sysex has no limit, instead being terminated by 0xF7, and thus returns None
    pub fn cmd_len(&self) -> Option<u8> {
        match self {
            ChannelStatus(NoteOff, _ch) => Some(3),
            ChannelStatus(NoteOn, _ch) => Some(3),
            ChannelStatus(Polyphonic, _ch) => Some(3),
            ChannelStatus(Control, _ch) => Some(3),
            ChannelStatus(Program, _ch) => Some(2),
            ChannelStatus(Pressure, _ch) => Some(2),
            ChannelStatus(PitchBend, _ch) => Some(3),

            SystemStatus(SysexStart) => None,

            SystemStatus(TimeCodeQuarterFrame) => Some(2),
            SystemStatus(SongPositionPointer) => Some(3),
            SystemStatus(SongSelect) => Some(2),
            SystemStatus(TuneRequest) => Some(1),

            SystemStatus(TimingClock) => Some(1),
            SystemStatus(MeasureEnd) => Some(2),
            SystemStatus(Start) => Some(1),
            SystemStatus(Continue) => Some(1),
            SystemStatus(Stop) => Some(1),
            SystemStatus(ActiveSensing) => Some(1),
            SystemStatus(SystemReset) => Some(1),
        }
    }
}

impl TryFrom<u8> for MidiStatus {
    type Error = MidiError;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        if byte < NoteOff as u8 {
            return Err(MidiError::NotAMidiStatus);
        }
        Ok(if byte < 0xF0 {
            ChannelStatus(
                ChannelCommand::try_from(byte & 0xF0)
                    .map_err(|_| MidiError::NotAChanelCommand)?,
                U4::try_from(byte & 0x0F)?,
            )
        } else {
            SystemStatus(SystemCommand::try_from(byte)
                .map_err(|_| MidiError::NotASystemCommand)?)
        })
    }
}

#[derive(Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum ChannelCommand {
    // Channel commands, lower bits of discriminants ignored (channel)
    NoteOff = 0x80,
    NoteOn = 0x90,
    Polyphonic = 0xA0,
    Control = 0xB0,
    Program = 0xC0,
    Pressure = 0xD0,
    PitchBend = 0xE0,
}

pub fn is_midi_status(byte: u8) -> bool {
    byte >= SysexStart as u8
}

/// Sysex sequence terminator, _not_ a status byte
pub const SYSEX_START: u8 = 0xF0;
/// Sysex sequence terminator, _not_ a status byte
pub const SYSEX_END: u8 = 0xF7;


#[derive(Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum SystemCommand {
    // System commands
    SysexStart = SYSEX_START,

    // System Common
    TimeCodeQuarterFrame = 0xF1,
    SongPositionPointer = 0xF2,
    SongSelect = 0xF3,
    TuneRequest = 0xF6,

    // System Realtime
    TimingClock = 0xF8,
    MeasureEnd = 0xF9,
    Start = 0xFA,
    Continue = 0xFB,
    Stop = 0xFC,
    ActiveSensing = 0xFE,
    SystemReset = 0xFF,
}

// const MAX_FRAGMENT_SIZE: usize = USB_BUFFER_SIZE.into();

// pub enum FragmentSource {
//     /// USB fragments carry additional header byte
//     USB,
//     /// Serial fragments reserve first byte for possible USB header
//     SERIAL,
// }
//
// pub struct MidiFragment {
//     source: FragmentSource,
//     bytes: [u8; MAX_FRAGMENT_SIZE],
// }
//
// impl MidiFragment {
//     pub fn as_usb_buffer(&mut self) -> &mut [u8] {
//         if self.source == SERIAL {
//             // set USB MIDI index code from status message
//             self.bytes[0] = CodeIndexNumber::from_command(self.get_command());
//         }
//         &mut self.bytes
//     }
//
//     pub fn as_serial_buffer(&mut self) -> &mut [u8] {
//         // skip usb header
//         &mut self.bytes[1..]
//     }
//
//     pub fn get_cable(&self) -> Cable {
//         Cable::from(self.bytes[0] >> 4)
//     }
//
//     pub fn get_status(&self) -> MidiStatus {
//         MidiStatus::from(self.bytes[1])
//     }
// }
