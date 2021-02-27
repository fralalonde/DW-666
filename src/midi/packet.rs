//! USB-MIDI Event Packet definitions
//! USB-MIDI is a superset of the MIDI protocol

use crate::midi::message::MidiMessage;
use crate::midi::u4::U4;
use core::convert::{TryFrom};
use crate::midi::MidiError;
use core::ops::{Deref};
use crate::midi::status::{MidiStatus, SystemCommand};
use CodeIndexNumber::*;
use MidiStatus::{ChannelStatus, SystemStatus};

pub type CableNumber = U4;

#[derive(Copy, Clone, Default)]
pub struct PartialPacket {
    bytes: [u8; 4]
}

impl PartialPacket {
    /// First byte temporarily used as payload length marker, set to proper Code Index Number upon build
    pub fn payload_len(&self) -> u8 {
        self.bytes[0]
    }

    pub fn cmd_len(&self) -> Result<Option<u8>, MidiError> {
        Ok(MidiStatus::try_from(self.bytes[1])?.cmd_len())
    }

    pub fn build(mut self, cable_number: CableNumber) -> Result<MidiPacket, MidiError> {
        let status = MidiStatus::try_from(self.bytes[1])?;
        self.bytes[0] = CodeIndexNumber::try_from(status)? as u8 | u8::from(cable_number) << 4;
        Ok(MidiPacket { bytes: self.bytes })
    }

    pub fn end_sysex(mut self, cable_number: CableNumber) -> Result<MidiPacket, MidiError> {
        self.bytes[0] = CodeIndexNumber::end_sysex(self.payload_len())? as u8 | u8::from(cable_number) << 4;
        Ok(MidiPacket { bytes: self.bytes })
    }
}

/// USB Event Packets are used to move MIDI across Serial and USB devices
pub struct PacketBuilder {
    pub prev_status: Option<u8>,
    pub pending_sysex: Option<PartialPacket>,
    inner: PartialPacket,
}

impl Deref for PacketBuilder {
    type Target = PartialPacket;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl PacketBuilder {
    pub fn new() -> Self {
        PacketBuilder { prev_status: None, pending_sysex: None, inner: PartialPacket::default() }
    }

    pub fn next(prev_status: Option<u8>, pending_sysex: Option<PartialPacket>) -> Self {
        PacketBuilder { prev_status, pending_sysex, inner: PartialPacket::default() }
    }

    pub fn push<T: Into<u8>>(&mut self, byte: T) {
        self.inner.bytes[self.inner.bytes[0] as usize] = byte.into();
        self.inner.bytes[0] += 1 // see payload_len()
    }
}

#[derive(Default, Debug)]
pub struct MidiPacket {
    bytes: [u8; 4]
}

impl MidiPacket {
    pub fn from_raw(bytes: [u8; 4]) -> Result<Self, MidiError> {
        Ok(MidiPacket { bytes })
    }

    pub fn from_message(cable_number: CableNumber, message: MidiMessage) -> Self {
        let mut packet = PacketBuilder::new();
        let status = MidiStatus::from(&message);
        packet.push(u8::from(status));
        match message {
            MidiMessage::NoteOff(_, note, vel) => {
                packet.push(note);
                packet.push(vel);
            }
            MidiMessage::NoteOn(_, note, vel) => {
                packet.push(note);
                packet.push(vel);
            }
            MidiMessage::NotePressure(_, note, pres) => {
                packet.push(note);
                packet.push(pres);
            }
            MidiMessage::ChannelPressure(_, pres) => {
                packet.push(pres);
            }
            MidiMessage::ProgramChange(_, patch) => {
                packet.push(patch);
            }
            MidiMessage::ControlChange(_, ctrl, val) => {
                packet.push(ctrl);
                packet.push(val);
            }
            MidiMessage::PitchBend(_, bend) => {
                let (lsb, msb) = bend.into();
                packet.push(lsb);
                packet.push(msb);
            }
            MidiMessage::TimeCodeQuarterFrame(val) => {
                packet.push(val);
            }
            MidiMessage::SongPositionPointer(p1, p2) => {
                packet.push(p1);
                packet.push(p2);
            }
            MidiMessage::SongSelect(song) => {
                packet.push(song);
            }
            // other messages are single byte (status only)
            _ => {}
        }
        // there is _no_ reason for this to be invalid
        packet.build(cable_number).unwrap()
    }


    pub fn payload(&self) -> Result<&[u8], MidiError> {
        let header = PacketHeader::try_from(self.bytes[0])?;
        Ok(&self.bytes[1..header.code_index_number.payload_len()])
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[allow(unused)]
#[derive(Debug)]
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
#[allow(unused)]
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
            ChannelStatus(cmd, _ch) => CodeIndexNumber::try_from(MidiStatus::try_from(cmd as u8)?)?,
            SystemStatus(SystemCommand::SysexStart) => Sysex,

            SystemStatus(SystemCommand::TimeCodeQuarterFrame) => SystemCommonLen2,
            SystemStatus(SystemCommand::SongPositionPointer) => SystemCommonLen3,
            SystemStatus(SystemCommand::TuneRequest) => SystemCommonLen1,
            SystemStatus(SystemCommand::SongSelect) => SystemCommonLen2,

            SystemStatus(SystemCommand::TimingClock) => SystemCommonLen1,
            SystemStatus(SystemCommand::MeasureEnd) => SystemCommonLen2,
            SystemStatus(SystemCommand::Start) => SystemCommonLen1,
            SystemStatus(SystemCommand::Continue) => SystemCommonLen1,
            SystemStatus(SystemCommand::Stop) => SystemCommonLen1,
            SystemStatus(SystemCommand::ActiveSensing) => SystemCommonLen1,
            SystemStatus(SystemCommand::SystemReset) => SystemCommonLen1,
        })
    }
}

impl CodeIndexNumber {
    fn end_sysex(len: u8) -> Result<CodeIndexNumber, MidiError> {
        match len {
            1 => Ok(SystemCommonLen1),
            2 => Ok(SysexEndsNext2),
            3 => Ok(SysexEndsNext3),
            _ => Err(MidiError::SysexOutOfBounds)
        }
    }

    pub fn payload_len(&self) -> usize {
        match self {
            MiscFunction => 0,
            CableEvents => 0,
            SystemCommonLen2 => 2,
            SystemCommonLen3 => 3,
            Sysex => 3,
            SystemCommonLen1 => 1,
            SysexEndsNext2 => 2,
            SysexEndsNext3 => 3,
            NoteOff => 3,
            NoteOn => 3,
            PolyKeypress => 3,
            ControlChange => 3,
            ProgramChange => 2,
            ChannelPressure => 2,
            PitchbendChange => 3,
            SingleByte => 1,
        }
    }
}
