use num_enum::UnsafeFromPrimitive;
use core::convert::TryFrom;
use crate::status::Status::*;
use crate::{Message, MidiError};
use crate::status::Status::{SongSelect, NoteOff, NoteOn, NotePressure};

pub const NOTE_OFF: u8 = 0x80;
pub const NOTE_ON: u8 = 0x90;
pub const NOTE_PRESSURE: u8 = 0xA0;
pub const CONTROL_CHANGE: u8 = 0xB0;
pub const PROGRAM_CHANGE: u8 = 0xC0;
pub const CHANNEL_PRESSURE: u8 = 0xD0;
pub const PITCH_BEND: u8 = 0xE0;

pub const SYSEX_START: u8 = 0xF0;

pub const TIME_CODE_QUARTER_FRAME: u8 = 0xF1;
pub const SONG_POSITION_POINTER: u8 = 0xF2;
pub const SONG_SELECT: u8 = 0xF3;
pub const TUNE_REQUEST: u8 = 0xF6;

pub const TIMING_CLOCK: u8 = 0xF8;
pub const MEASURE_END: u8 = 0xF9;
pub const START: u8 = 0xFA;
pub const CONTINUE: u8 = 0xFB;
pub const STOP: u8 = 0xFC;
pub const ACTIVE_SENSING: u8 = 0xFE;
pub const SYSTEM_RESET: u8 = 0xFF;

/// Sysex sequence terminator. NOT a status byte.
pub const SYSEX_END: u8 = 0xF7;

pub fn is_non_status(byte: u8) -> bool {
    byte < NOTE_OFF || byte == SYSEX_END
}

pub fn is_channel_status(byte: u8) -> bool {
    (NOTE_OFF..SYSEX_START).contains(&byte)
}

#[derive(Copy, Clone, Debug, UnsafeFromPrimitive, Eq, PartialEq)]
#[repr(u8)]
pub enum Status {
    // Channel commands, lower bits of discriminants ignored (channel)
    NoteOff = NOTE_OFF,
    NoteOn = NOTE_ON,
    NotePressure = NOTE_PRESSURE,
    ControlChange = CONTROL_CHANGE,
    ProgramChange = PROGRAM_CHANGE,
    ChannelPressure = CHANNEL_PRESSURE,
    PitchBend = PITCH_BEND,

    // System commands
    SysexStart = SYSEX_START,

    // System Common
    TimeCodeQuarterFrame = TIME_CODE_QUARTER_FRAME,
    SongPositionPointer = SONG_POSITION_POINTER,
    SongSelect = SONG_SELECT,
    TuneRequest = TUNE_REQUEST,

    // System Realtime
    TimingClock = TIMING_CLOCK,
    MeasureEnd = MEASURE_END,
    Start = START,
    Continue = CONTINUE,
    Stop = STOP,
    ActiveSensing = ACTIVE_SENSING,
    SystemReset = SYSTEM_RESET,
}

pub fn status_byte(msg: &Message) -> Option<u8> {
    match msg {
        Message::NoteOff(ch, ..) => Some(Status::NoteOff as u8 + ch.0),
        Message::NoteOn(ch, ..) => Some(Status::NoteOn as u8 + ch.0),
        Message::NotePressure(ch, ..) => Some(Status::NotePressure as u8 + ch.0),
        Message::ChannelPressure(ch, ..) => Some(Status::ChannelPressure as u8 + ch.0),
        Message::ProgramChange(ch, ..) => Some(Status::ProgramChange as u8 + ch.0),
        Message::ControlChange(ch, ..) => Some(Status::ControlChange as u8 + ch.0),
        Message::PitchBend(ch, ..) => Some(Status::PitchBend as u8 + ch.0),

        Message::TimeCodeQuarterFrame(_) => Some(Status::TimeCodeQuarterFrame as u8),
        Message::SongPositionPointer(_, _) => Some(Status::SongPositionPointer as u8),
        Message::SongSelect(_) => Some(Status::SongSelect as u8),
        Message::TuneRequest => Some(Status::TuneRequest as u8),
        Message::TimingClock => Some(Status::TimingClock as u8),
        Message::Start => Some(Status::Start as u8),
        Message::Continue => Some(Status::Continue as u8),
        Message::Stop => Some(Status::Stop as u8),
        Message::ActiveSensing => Some(Status::ActiveSensing as u8),
        Message::SystemReset => Some(Status::SystemReset as u8),
        Message::MeasureEnd(_) => Some(Status::MeasureEnd as u8),
        _ => None,
    }
}


impl Status {
    /// Returns expected size in bytes of associated MIDI message
    /// Including the status byte itself
    /// Sysex has no limit, instead being terminated by 0xF7, and thus returns 3 (max packet length)
    pub fn expected_len(&self) -> u8 {
        match self {
            SysexStart => 3,
            TuneRequest | TimingClock | Start | Continue | Stop | ActiveSensing | SystemReset => 1,
            NoteOff | NoteOn | NotePressure | ControlChange | PitchBend | SongPositionPointer => 3,
            ProgramChange | ChannelPressure | TimeCodeQuarterFrame | SongSelect | MeasureEnd => 2,
        }
    }
}

impl TryFrom<u8> for Status {
    type Error = MidiError;

    fn try_from(mut byte: u8) -> Result<Self, Self::Error> {
        if is_non_status(byte) {
            return Err(MidiError::InvalidStatus(byte));
        }
        if is_channel_status(byte) {
            // nuke channel bits
            byte &= 0xF0
        }
        Ok(unsafe { Status::from_unchecked(byte) })
    }
}
