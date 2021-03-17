//! USB-MIDI Event Packet definitions
//! USB-MIDI is a superset of the MIDI protocol

use crate::midi::message::RealtimeMessage;
use crate::midi::u4::U4;
use core::convert::{TryFrom};
use crate::midi::{MidiError};
use crate::midi::status::{MidiStatus, SystemStatus};
use CodeIndexNumber::*;
use MidiStatus::{Channel, System};

use num_enum::UnsafeFromPrimitive;

pub type CableNumber = U4;

#[derive(Default, Clone, Copy, Debug)]
pub struct MidiPacket {
    bytes: [u8; 4]
}

impl MidiPacket {
    pub fn from_raw(bytes: [u8; 4]) -> Self {
        MidiPacket { bytes }
    }

    pub fn cable_number(&self) -> Result<CableNumber, MidiError> {
        Ok(CableNumber::try_from(self.bytes[0] >> 4)?)
    }

    pub fn code_index_number(&self) -> CodeIndexNumber {
        CodeIndexNumber::from(self.bytes[0] & 0x0F)
    }

    pub fn status(&self) -> Option<MidiStatus> {
        MidiStatus::try_from(self.payload()[0]).ok()
    }

    pub fn payload(&self) -> &[u8] {
        let cin = self.code_index_number();
        &self.bytes[1..cin.payload_len() + 1]
    }

    pub fn with_cable_num(mut self, cable_number: CableNumber) -> Self {
        self.bytes[0] = self.bytes[0] & 0x0F | u8::from(cable_number) << 4;
        self
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl From<RealtimeMessage> for MidiPacket {
    fn from(message: RealtimeMessage) -> Self {
        let mut packet = [0; 4];
        let status = MidiStatus::from(&message);
        let code_index_number = CodeIndexNumber::from(status);
        packet[0] = code_index_number as u8;
        packet[1] = u8::from(status);
        match message {
            RealtimeMessage::NoteOff(_, note, vel) => {
                packet[2] = note as u8;
                packet[3] = u8::from(vel);
            }
            RealtimeMessage::NoteOn(_, note, vel) => {
                packet[2] = note as u8;
                packet[3] = u8::from(vel);
            }
            RealtimeMessage::NotePressure(_, note, pres) => {
                packet[2] = note as u8;
                packet[3] = u8::from(pres);
            }
            RealtimeMessage::ChannelPressure(_, pres) => {
                packet[2] = u8::from(pres);
            }
            RealtimeMessage::ProgramChange(_, patch) => {
                packet[2] = u8::from(patch);
            }
            RealtimeMessage::ControlChange(_, ctrl, val) => {
                packet[2] = u8::from(ctrl);
                packet[3] = u8::from(val);
            }
            RealtimeMessage::PitchBend(_, bend) => {
                let (lsb, msb) = bend.into();
                packet[2] = u8::from(lsb);
                packet[3] = u8::from(msb);
            }
            RealtimeMessage::TimeCodeQuarterFrame(val) => {
                packet[2] = u8::from(val);
            }
            RealtimeMessage::SongPositionPointer(p1, p2) => {
                packet[2] = u8::from(p1);
                packet[3] = u8::from(p2);
            }
            RealtimeMessage::SongSelect(song) => {
                packet[2] = u8::from(song);
            }
            // other messages are single byte (status only)
            _ => {}
        }
        // there is _no_ reason for this to be invalid
        Self::from_raw(packet)
    }
}

/// The Code Index Number(CIN) indicates the classification
/// of the bytes in the MIDI_x fields
#[allow(unused)]
#[derive(Debug, Eq, PartialEq, UnsafeFromPrimitive)]
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

impl From<u8> for CodeIndexNumber {
    fn from(byte: u8) -> Self {
        unsafe {CodeIndexNumber::from_unchecked(byte & 0x0F)}
    }
}

impl From<MidiStatus> for CodeIndexNumber {
    fn from(status: MidiStatus) -> Self {
        match status {
            Channel(cmd, _ch) => CodeIndexNumber::try_from((cmd as u8 >> 4) as u8).unwrap(),

            System(SystemStatus::SysexStart) => Sysex,

            System(SystemStatus::TimeCodeQuarterFrame) => SystemCommonLen2,
            System(SystemStatus::SongPositionPointer) => SystemCommonLen3,
            System(SystemStatus::TuneRequest) => SystemCommonLen1,
            System(SystemStatus::SongSelect) => SystemCommonLen2,

            System(SystemStatus::TimingClock) => SystemCommonLen1,
            System(SystemStatus::MeasureEnd) => SystemCommonLen2,
            System(SystemStatus::Start) => SystemCommonLen1,
            System(SystemStatus::Continue) => SystemCommonLen1,
            System(SystemStatus::Stop) => SystemCommonLen1,
            System(SystemStatus::ActiveSensing) => SystemCommonLen1,
            System(SystemStatus::SystemReset) => SystemCommonLen1,
        }
    }
}

impl CodeIndexNumber {

    pub fn end_sysex(len: usize) -> Result<CodeIndexNumber, MidiError> {
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
