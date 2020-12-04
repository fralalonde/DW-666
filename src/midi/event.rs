//! USB-MIDI Event Packet definitions
//! USB-MIDI is a superset of the MIDI protocol

use crate::midi::message::{MidiStatus, SystemCommand};
use crate::midi::u4::U4;
use core::convert::{TryFrom, TryInto};
use crate::midi::MidiError;
use core::ops::{Deref};
use defmt::Format;

pub type CableNumber = U4;

#[derive(Copy, Clone, Default)]
pub struct PacketB {
    bytes: [u8; 4]
}

impl PacketB {
    /// First byte temporarily used as payload length marker, set to proper Code Index Number upon build
    pub fn payload_len(&self) -> u8 {
        self.bytes[0]
    }

    pub fn build(mut self) -> Result<Packet, MidiError> {
        let status = MidiStatus::try_from(self.bytes[1])?;
        self.bytes[0] = CodeIndexNumber::try_from(status)? as u8;
        Ok(Packet { bytes: self.bytes })
    }

    pub fn end_sysex(mut self) -> Result<Packet, MidiError> {
        self.bytes[0] = CodeIndexNumber::end_sysex(self.payload_len())? as u8;
        Ok(Packet { bytes: self.bytes })
    }

    pub fn cmd_len(&self) -> Result<Option<u8>, MidiError> {
        Ok(MidiStatus::try_from(self.bytes[1])?.cmd_len())
    }
}

/// Convert serial byte stream into USB sized packets
pub struct PacketBuilder {
    pub prev_status: Option<u8>,
    pub pending_sysex: Option<PacketB>,
    inner: PacketB,
}

impl Deref for PacketBuilder {
    type Target = PacketB;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl PacketBuilder {
    pub fn new() -> Self {
        PacketBuilder { prev_status: None, pending_sysex: None, inner: PacketB::default() }
    }

    pub fn next(prev_status: Option<u8>, pending_sysex: Option<PacketB>) -> Self {
        PacketBuilder { prev_status, pending_sysex, inner: PacketB::default() }
    }

    pub fn push(&mut self, byte: u8) {
        self.inner.bytes[self.inner.bytes[0] as usize] = byte;
        self.inner.bytes[0] += 1 // see payload_len()
    }
}

#[derive(Default, Format)]
pub struct Packet {
    bytes: [u8; 4]
}

impl Packet {
    pub fn from_raw(bytes: [u8; 4]) -> Result<Self, MidiError> {
        Ok(Packet { bytes })
    }

    pub fn payload(&self) -> Result<&[u8], MidiError> {
        let header = PacketHeader::try_from(self.bytes[0])?;
        Ok(&self.bytes[1..header.code_index_number.payload_len()])
    }

    pub fn raw(&self) -> &[u8] {
        &self.bytes
    }
}

pub struct PacketHeader {
    cable_number: CableNumber,
    code_index_number: CodeIndexNumber,
}

impl TryFrom<u8> for PacketHeader {
    type Error = MidiError;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        Ok(PacketHeader {
            cable_number: U4::try_from(byte)?,
            code_index_number: CodeIndexNumber::try_from(MidiStatus::try_from(byte)?)?,
        })
    }
}

/// The Code Index Number(CIN) indicates the classification
/// of the bytes in the MIDI_x fields
#[derive(Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CodeIndexNumber {
    /// Miscellaneous function codes. Reserved for future extensions
    MiscFunction = 0x00,
    /// Cable events. Reserved for future expansion.
    CableEvents = 0x1,
    /// Two-byte System Common messages like MTC, SongSelect, etc.
    SystemCommonLen2 = 0x2,
    /// Three-byte System Common messages like SPP, etc.
    SystemCommonLen3 = 0x3,
    /// SysEx starts or continues
    Sysex = 0x4,
    /// Single-byte System Common Message or SysEx ends with following single byte.
    SystemCommonLen1 = 0x5,
    /// SysEx ends with following two bytes
    SysexEndsNext2 = 0x6,
    /// SysEx ends with following three bytes
    SysexEndsNext3 = 0x7,

    /// Note Off
    NoteOff = 0x8,
    /// Note On
    NoteOn = 0x9,
    /// Poly-KeyPess
    PolyKeypress = 0xA,
    /// Control Change
    ControlChange = 0xB,
    /// Program Change
    ProgramChange = 0xC,
    /// Channel Pressure
    ChannelPressure = 0xD,
    /// Pitch Bend Change
    PitchbendChange = 0xE,

    /// Single Byte
    SingleByte = 0xF,
}

impl TryFrom<MidiStatus> for CodeIndexNumber {
    type Error = MidiError;

    fn try_from(status: MidiStatus) -> Result<Self, Self::Error> {
        Ok(match status {
            MidiStatus::ChannelStatus(cmd, _ch) => CodeIndexNumber::try_from(MidiStatus::try_from(cmd as u8)?)?,
            MidiStatus::SystemStatus(SystemCommand::SysexStart) => CodeIndexNumber::Sysex,

            MidiStatus::SystemStatus(SystemCommand::TimeCodeQuarterFrame) => CodeIndexNumber::SystemCommonLen2,
            MidiStatus::SystemStatus(SystemCommand::SongPositionPointer) => CodeIndexNumber::SystemCommonLen3,
            MidiStatus::SystemStatus(SystemCommand::TuneRequest) => CodeIndexNumber::SystemCommonLen1,
            MidiStatus::SystemStatus(SystemCommand::SongSelect) => CodeIndexNumber::SystemCommonLen2,

            MidiStatus::SystemStatus(SystemCommand::TimingClock) => CodeIndexNumber::SystemCommonLen1,
            MidiStatus::SystemStatus(SystemCommand::MeasureEnd) => CodeIndexNumber::SystemCommonLen2,
            MidiStatus::SystemStatus(SystemCommand::Start) => CodeIndexNumber::SystemCommonLen1,
            MidiStatus::SystemStatus(SystemCommand::Continue) => CodeIndexNumber::SystemCommonLen1,
            MidiStatus::SystemStatus(SystemCommand::Stop) => CodeIndexNumber::SystemCommonLen1,
            MidiStatus::SystemStatus(SystemCommand::ActiveSensing) => CodeIndexNumber::SystemCommonLen1,
            MidiStatus::SystemStatus(SystemCommand::SystemReset) => CodeIndexNumber::SystemCommonLen1,
        })
    }
}

impl CodeIndexNumber {
    fn end_sysex(len: u8) -> Result<CodeIndexNumber, MidiError> {
        match len {
            1 => Ok(CodeIndexNumber::SystemCommonLen1),
            2 => Ok(CodeIndexNumber::SysexEndsNext2),
            3 => Ok(CodeIndexNumber::SysexEndsNext3),
            _ => Err(MidiError::SysexOutOfBounds)
        }
    }

    pub fn payload_len(&self) -> usize {
        match self {
            CodeIndexNumber::MiscFunction => 0,
            CodeIndexNumber::CableEvents => 0,
            CodeIndexNumber::SystemCommonLen2 => 2,
            CodeIndexNumber::SystemCommonLen3 => 3,
            CodeIndexNumber::Sysex => 3,
            CodeIndexNumber::SystemCommonLen1 => 1,
            CodeIndexNumber::SysexEndsNext2 => 2,
            CodeIndexNumber::SysexEndsNext3 => 3,
            CodeIndexNumber::NoteOff => 3,
            CodeIndexNumber::NoteOn => 3,
            CodeIndexNumber::PolyKeypress => 3,
            CodeIndexNumber::ControlChange => 3,
            CodeIndexNumber::ProgramChange => 2,
            CodeIndexNumber::ChannelPressure => 2,
            CodeIndexNumber::PitchbendChange => 3,
            CodeIndexNumber::SingleByte => 1,
        }
    }
}
