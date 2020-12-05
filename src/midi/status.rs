use num_enum::TryFromPrimitive;
use core::convert::TryFrom;
use crate::midi::MidiError;
use crate::midi::status::MidiStatus::{SystemStatus, ChannelStatus};
use crate::midi::status::ChannelCommand::NoteOff;
use crate::midi::message::{MidiMessage, Channel};
use crate::midi::u4::U4;
use self::ChannelCommand::*;
use self::SystemCommand::*;

pub enum MidiStatus {
    ChannelStatus(ChannelCommand, Channel),
    SystemStatus(SystemCommand),
}

impl From<&MidiMessage> for MidiStatus {
    fn from(msg: &MidiMessage) -> Self {
        match msg {
            MidiMessage::NoteOff(channel, _, _) => ChannelStatus(NoteOff, *channel),
            MidiMessage::NoteOn(channel, _, _) => ChannelStatus(NoteOn, *channel),
            MidiMessage::NotePressure(channel, _, _) => ChannelStatus(NotePressure, *channel),
            MidiMessage::ChannelPressure(channel, _) => ChannelStatus(ChannelPressure, *channel),
            MidiMessage::ProgramChange(channel, _) => ChannelStatus(Program, *channel),
            MidiMessage::ControlChange(channel, _, _) => ChannelStatus(Control, *channel),
            MidiMessage::PitchBend(channel, _) => ChannelStatus(PitchBend, *channel),
            MidiMessage::TimeCodeQuarterFrame(_) => SystemStatus(TimeCodeQuarterFrame),
            MidiMessage::SongPositionPointer(_, _) => SystemStatus(SongPositionPointer),
            MidiMessage::SongSelect(_) => SystemStatus(SongSelect),
            MidiMessage::TuneRequest => SystemStatus(TuneRequest),
            MidiMessage::TimingClock => SystemStatus(TimingClock),
            MidiMessage::Start => SystemStatus(Start),
            MidiMessage::Continue => SystemStatus(Continue),
            MidiMessage::Stop => SystemStatus(Stop),
            MidiMessage::ActiveSensing => SystemStatus(ActiveSensing),
            MidiMessage::SystemReset => SystemStatus(SystemReset),
        }
    }
}

impl MidiStatus {
    /// Returns expected size in bytes of associated MIDI message
    /// Including the status byte itself
    /// Sysex has no limit, instead being terminated by 0xF7, and thus returns None
    pub fn cmd_len(&self) -> Option<u8> {
        match self {
            ChannelStatus(NoteOff, _ch) => Some(3),
            ChannelStatus(NoteOn, _ch) => Some(3),
            ChannelStatus(NotePressure, _ch) => Some(3),
            ChannelStatus(Control, _ch) => Some(3),
            ChannelStatus(Program, _ch) => Some(2),
            ChannelStatus(ChannelPressure, _ch) => Some(2),
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

impl From<MidiStatus> for u8 {
    fn from(status: MidiStatus) -> Self {
        match status {
            ChannelStatus(cmd, ch) => cmd as u8 | u8::from(ch),
            SystemStatus(cmd) => cmd as u8,
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
    NotePressure = 0xA0,
    Control = 0xB0,
    Program = 0xC0,
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
