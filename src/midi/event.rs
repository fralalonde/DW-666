//! USB-MIDI Event Packet definitions
//! USB-MIDI is a superset of the MIDI protocol

use crate::midi::message::{MidiStatus, SystemCommand, SYSEX_END, is_midi_status};
use crate::midi::u4::U4;
use core::convert::TryFrom;
use stm32f1xx_hal::pac::CAN1;
use alloc::vec::Vec;
use crate::midi::MidiError;
use crate::midi::MidiError::PayloadOverflow;
use crate::midi::message::SystemCommand::SysexStart;
use alloc::boxed::Box;

pub type CableNumber = U4;

/// Convert serial byte stream into USB sized packets
pub struct PacketBuilder{
    prev_status: Option<u8>,
    pending_sysex: Option<Box<PacketBuilder>>,
    bytes: [u8;4]
}

impl PacketBuilder {

    pub fn new() -> Self {
        PacketBuilder { prev_status: None, pending_sysex: None, bytes: [0;4] }
    }

    pub fn next(prev_status: Option<u8>, pending_sysex: Option<Box<PacketBuilder>>,) -> Self {
        PacketBuilder { prev_status, pending_sysex, bytes: [0;4]}
    }

    /// First byte temporarily used as payload length marker, set to proper Code Index Number upon build
    fn payload_len(&self) -> u8 {
        self.bytes[0]
    }

    fn push(&mut self, byte: u8) {
        self.bytes[self.bytes[0]] = byte;
        self.bytes[0] += 1 // see len()
    }

    fn build(mut self) -> Result<Packet, MidiError> {
        let status = MidiStatus::from(self.bytes[1])?;
        self.bytes[0] = CodeIndexNumber::try_from(status)?;
        Ok(Packet { bytes: self.bytes} )
    }

    fn end_sysex(mut self) -> Result<Packet, MidiError> {
        self.bytes[0] = CodeIndexNumber::end_sysex(self.payload_len())?;
        Ok(Packet { bytes: self.bytes} )
    }

    /// Push new command payload byt
    /// returns:
    /// - Ok(false) if packet is incomplete
    /// - Ok(true) if packet is complete - should not be pushed to anymore, waiting on either sysex or sysex_end
    /// - MidiError(PacketOverflow) if packet was complete
    pub fn advance(mut self, byte: u8) -> Result<(PacketBuilder, Option<Packet>), MidiError> {
        match (self.payload_len(), byte, &self.pending_sysex, self.prev_status) {
            (0, SYSEX_END, Some(pending), _) => {
                // end of sysex stream, release previous packet as final
                self.pending_sysex = None;
                Ok((self, Some(pending.end_sysex()?)))
            }
            (_, SYSEX_END, None, Some(status)) if status == SysexStart as u8 => {
                // ignore sysex end outside sysex context
                Ok((PacketBuilder::new(), Some(self.end_sysex()?)))
            }
            (_, SYSEX_END, None, _) => {
                // ignore sysex end outside sysex context
                Ok((self, None))
            }
            (_, byte, Some(_), _) if is_midi_status(byte) => {
                Err(MidiError::SysexInterrupted)
            }
            (0, byte, Some(pending), _) => {
                // continue sysex stream, release previous packet normally
                self.push(byte);
                self.pending_sysex = None;
                Ok((self, Some(pending.build()?)))
            }
            (0, byte, None, Some(prev_status)) if !is_midi_status(byte) => {
                // repeating status (MIDI protocol optimisation)
                self.push(prev_status);
                self.push(byte);
            }
            (0, byte, None, None) if !is_midi_status(byte) => {
                // first byte of non-sysex payload should be a status byte
                Err(MidiError::NotAMidiStatus)
            }
            (0, byte, None, _) if is_midi_status(byte) => {
                // regular status byte starting new command
                self.prev_status = Some(byte);
                self.push(byte);
            }
            (3, _, _, _) => {
                // can't add more, packet should have been released, probable bug in impl
                Err(MidiError::PayloadOverflow)
            }
            (2, _, None, Some(SysexStart)) => {
                // sysex packet complete, wait for next byte before release with correct CIN
                self.push(byte);
                Ok((PacketBuilder::next(self.prev_status, Some(Box::new(self))), None))
            }
            (payload_len, _, None, _) if payload_len > 1 => {
                // release current packet if complete
                self.push(byte);
                let cmd_len = MidiStatus::try_from(self.bytes[1])?.cmd_len();
                if let Some(cmd_len) = cmd_len {
                    if cmd_len == payload_len {
                        Ok((PacketBuilder::next(self.prev_status, None), Some(self.build()?)))
                    } else {
                        Ok((self, None))
                    }
                }
            }
            _ => {
                Err(MidiError::UnhandledDecode)
            }
        }
    }
}

#[derive(Default)]
pub struct Packet{
    bytes: [u8; 4]
}

impl Packet {
    pub fn payload(&self) -> &[u8] {
        let header = PacketHeader::from(self.0[0]);
        self.0[1..header.code_index_number.get_payload_size()]
    }
}

pub struct PacketHeader {
    cable_number: CableNumber,
    code_index_number: CodeIndexNumber,
}

impl From<u8> for PacketHeader {
    fn from(byte: u8) -> Self {
        PacketHeader {
            cable_number: U4::try_from(byte).unwrap(),
            code_index_number: CodeIndexNumber::try_from(byte & 0x0F).unwrap(),
        }
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
            MidiStatus::ChannelStatus(cmd, _ch) => CodeIndexNumber::try_from(cmd as u8)?,
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
            _ => Err(MidiError::SysexOutofBounds)
        }
    }

    pub fn get_payload_size(&self) -> usize {
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
