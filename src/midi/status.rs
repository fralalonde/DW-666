use num_enum::TryFromPrimitive;
use core::convert::TryFrom;
use crate::midi::{MidiError, message};
use crate::midi::status::MidiStatus::{System, Channel};
use crate::midi::message::{RealtimeMessage};
use crate::midi::u4::U4;
use self::ChannelStatus::*;
use self::SystemStatus::*;

#[derive(Copy, Clone, Debug)]
pub enum MidiStatus {
    Channel(ChannelStatus, message::Channel),
    System(SystemStatus),
}

impl From<&RealtimeMessage> for MidiStatus {
    fn from(msg: &RealtimeMessage) -> Self {
        match msg {
            RealtimeMessage::NoteOff(channel, _, _) => Channel(NoteOff, *channel),
            RealtimeMessage::NoteOn(channel, _, _) => Channel(NoteOn, *channel),
            RealtimeMessage::NotePressure(channel, _, _) => Channel(NotePressure, *channel),
            RealtimeMessage::ChannelPressure(channel, _) => Channel(ChannelPressure, *channel),
            RealtimeMessage::ProgramChange(channel, _) => Channel(ProgramChange, *channel),
            RealtimeMessage::ControlChange(channel, _, _) => Channel(ControlChange, *channel),
            RealtimeMessage::PitchBend(channel, _) => Channel(PitchBend, *channel),
            RealtimeMessage::TimeCodeQuarterFrame(_) => System(TimeCodeQuarterFrame),
            RealtimeMessage::SongPositionPointer(_, _) => System(SongPositionPointer),
            RealtimeMessage::SongSelect(_) => System(SongSelect),
            RealtimeMessage::TuneRequest => System(TuneRequest),
            RealtimeMessage::TimingClock => System(TimingClock),
            RealtimeMessage::Start => System(Start),
            RealtimeMessage::Continue => System(Continue),
            RealtimeMessage::Stop => System(Stop),
            RealtimeMessage::ActiveSensing => System(ActiveSensing),
            RealtimeMessage::SystemReset => System(SystemReset),
            RealtimeMessage::MeasureEnd(_) => System(MeasureEnd),
        }
    }
}

impl MidiStatus {
    /// Returns expected size in bytes of associated MIDI message
    /// Including the status byte itself
    /// Sysex has no limit, instead being terminated by 0xF7, and thus returns None
    pub fn cmd_len(&self) -> Option<usize> {
        match self {
            Channel(NoteOff, _ch) => Some(3),
            Channel(NoteOn, _ch) => Some(3),
            Channel(NotePressure, _ch) => Some(3),
            Channel(ControlChange, _ch) => Some(3),
            Channel(ProgramChange, _ch) => Some(2),
            Channel(ChannelPressure, _ch) => Some(2),
            Channel(PitchBend, _ch) => Some(3),

            System(SysexStart) => None,

            System(TimeCodeQuarterFrame) => Some(2),
            System(SongPositionPointer) => Some(3),
            System(SongSelect) => Some(2),
            System(TuneRequest) => Some(1),

            System(TimingClock) => Some(1),
            System(MeasureEnd) => Some(2),
            System(Start) => Some(1),
            System(Continue) => Some(1),
            System(Stop) => Some(1),
            System(ActiveSensing) => Some(1),
            System(SystemReset) => Some(1),
        }
    }
}

impl From<MidiStatus> for u8 {
    fn from(status: MidiStatus) -> Self {
        match status {
            Channel(cmd, ch) => cmd as u8 | u8::from(ch),
            System(cmd) => cmd as u8,
        }
    }
}

impl TryFrom<u8> for MidiStatus {
    type Error = MidiError;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        if byte < NoteOff as u8 {
            return Err(MidiError::NotAMidiStatus(byte));
        }
        Ok(if byte < 0xF0 {
            Channel(
                ChannelStatus::try_from(byte & 0xF0)
                    .map_err(|_| MidiError::NotAChannelStatus(byte))?,
                U4::try_from(byte & 0x0F)?,
            )
        } else {
            System(SystemStatus::try_from(byte)
                .map_err(|_| MidiError::NotASystemStatus(byte))?)
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum ChannelStatus {
    // Channel commands, lower bits of discriminants ignored (channel)
    NoteOff = 0x80,
    NoteOn = 0x90,
    NotePressure = 0xA0,
    ControlChange = 0xB0,
    ProgramChange = 0xC0,
    ChannelPressure = 0xD0,
    PitchBend = 0xE0,
}

pub fn is_midi_status(byte: u8) -> bool {
    byte >= SYSEX_START as u8
}

/// Sysex sequence terminator, _not_ a status byte
pub const SYSEX_START: u8 = 0xF0;
/// Sysex sequence terminator, _not_ a status byte
pub const SYSEX_END: u8 = 0xF7;

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum SystemStatus {
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
