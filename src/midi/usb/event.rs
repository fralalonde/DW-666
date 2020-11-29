use core::convert::TryFrom;
use crate::midi::u4::U4;
use crate::midi::message::{ChannelMessage, MidiStatus};
use stm32f1xx_hal::pac::CAN1;
use crate::midi::usb::device::Cable;

pub type CableNumber = U4;

pub struct PacketHeader {
    cable_number: CableNumber,
    code_index_number: CodeIndexNumber,
}

impl From<u8> for PacketHeader {
    fn from(byte: u8) -> Self {
        PacketHeader {
            cable_number: U4::try_from(byte).unwrap(),
            code_index_number: CodeIndexNumber::try_from(byte & 0x0F).unwrap()
        }
    }
}

/// The Code Index Number(CIN) indicates the classification 
/// of the bytes in the MIDI_x fields
#[derive(Debug, Eq, PartialEq, TryFromPrimitive)]
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
    SingleByte = 0xF
}

impl CodeIndexNumber {
    pub fn from_status(status: MidiStatus) -> CodeIndexNumber{
        CodeIndexNumber::try_from(status && 0xF0).unwrap()
    }
}
