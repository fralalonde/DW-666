//! Parse midi messages
use crate::midi::message::MidiCommand;
use crate::midi::notes::Note;
use crate::midi::{Channel, Control, Status};
use alloc::vec::Vec;

/// Keeps state for parsing Midi messages
#[derive(Debug, Clone, PartialEq)]
pub struct Parser {
    state: ParserState,
}

// /// Represents midi messages
// /// Note: not current exhaustive and SysEx messages end up
// /// being a confusing case. So are currently note implemented
// /// they are sort-of unbounded
pub enum ChannelMessage {
    NoteOff(Channel,Note,Velocity),
    NoteOn(Channel,Note,Velocity),
    PolyphonicAftertouch(Channel,Note,U7),
    ProgramChange(Channel,U7),
    ChannelAftertouch(Channel,U7),
    PitchWheelChange(Channel,U7,U7),
    KeyPressure(Channel,Note,U7),
    ControlChange(Channel, Control, U7),
    ChannelPressure(channel, value),
    PitchBendChange(channel, value),
}

#[derive(Debug, Clone, PartialEq)]
enum ParserState {
    Idle,
    NoteOnRecvd(Channel),
    NoteOnNoteRecvd(Channel, Note),

    NoteOffRecvd(Channel),
    NoteOffNoteRecvd(Channel, Note),

    KeyPressureRecvd(Channel),
    KeyPressureNoteRecvd(Channel, Note),

    ControlChangeRecvd(Channel),
    ControlChangeControlRecvd(Channel, Control),

    ProgramChangeRecvd(Channel),

    ChannelPressureRecvd(Channel),

    PitchBendRecvd(Channel),
    PitchBendFirstByteRecvd(Channel, u8),

    QuarterFrameRecvd,

    SongPositionRecvd,
    SongPositionLsbRecvd(u8),

    SongSelectRecvd,
}

/// Check if most significant bit is set which signifies a Midi status byte
fn is_status_byte(byte: u8) -> bool {
    byte & 0x80 == 0x80
}

/// Check if a byte corresponds to 0x1111xxxx which signifies either a system common or realtime message
fn is_system_message(byte: u8) -> bool {
    byte & 0xf0 == 0xf0
}

/// Split the message and channel part of a channel voice message
fn split_message_and_channel(byte: u8) -> (MidiCommand, Channel) {
    (byte & 0xF0, (byte & 0x0fu8).into())
}

/// State machine for parsing Midi data, can be fed bytes one-by-one, and returns parsed Midi
/// messages whenever one is completed.
impl Parser {
    /// Initialize midiparser state
    pub fn new() -> Self {
        Parser {
            state: ParserState::Idle,
        }
    }

    /// Parse midi event byte by byte. Call this whenever a byte is received. When a midi-event is
    /// completed it is returned, otherwise this method updates the internal midiparser state and
    /// and returns none.
    pub fn advance(&mut self, byte: u8) -> Option<MidiMessage> {
        if is_status_byte(byte) {
            if is_system_message(byte) {
                match byte {
                    // System common messages, these should reset parsing other messages
                    0xf0 => {
                        self.state = ParserState::Idle;
                        None
                    }
                    0xf1 => {
                        self.state = ParserState::QuarterFrameRecvd;
                        None
                    }
                    0xf2 => {
                        self.state = ParserState::SongPositionRecvd;
                        None
                    }
                    0xf3 => {
                        self.state = ParserState::SongSelectRecvd;
                        None
                    }
                    0xf6 => {
                        self.state = ParserState::Idle;
                        Some(MidiMessage::TuneRequest)
                    }
                    0xf7 => {
                        self.state = ParserState::Idle;
                        None
                    }

                    // System realtime messages
                    0xf8 => Some(MidiMessage::TimingClock),
                    0xf9 => None, // Reserved
                    0xfa => Some(MidiMessage::Start),
                    0xfb => Some(MidiMessage::Continue),
                    0xfc => Some(MidiMessage::Stop),
                    0xfd => None, // Reserved
                    0xfe => Some(MidiMessage::ActiveSensing),
                    0xff => Some(MidiMessage::Reset),

                    _ => {
                        // Undefined messages like 0xf4 and should end up here
                        self.state = ParserState::Idle;
                        None
                    }
                }
            } else {
                // Channel voice message

                let (message, channel) = split_message_and_channel(byte);

                match message {
                    0x80 => {
                        self.state = ParserState::NoteOffRecvd(channel);
                        None
                    }
                    0x90 => {
                        self.state = ParserState::NoteOnRecvd(channel);
                        None
                    }
                    0xA0 => {
                        self.state = ParserState::KeyPressureRecvd(channel);
                        None
                    }
                    0xB0 => {
                        self.state = ParserState::ControlChangeRecvd(channel);
                        None
                    }
                    0xC0 => {
                        self.state = ParserState::ProgramChangeRecvd(channel);
                        None
                    }
                    0xD0 => {
                        self.state = ParserState::ChannelPressureRecvd(channel);
                        None
                    }
                    0xE0 => {
                        self.state = ParserState::PitchBendRecvd(channel);
                        None
                    }
                    _ => None,
                }
            }
        } else {
            match self.state {
                ParserState::NoteOffRecvd(channel) => {
                    self.state = ParserState::NoteOffNoteRecvd(channel, byte.into());
                    None
                }
                ParserState::NoteOffNoteRecvd(channel, note) => {
                    self.state = ParserState::NoteOffRecvd(channel);
                    Some(MidiMessage::NoteOff(channel, note, byte.into()))
                }

                ParserState::NoteOnRecvd(channel) => {
                    self.state = ParserState::NoteOnNoteRecvd(channel, byte.into());
                    None
                }
                ParserState::NoteOnNoteRecvd(channel, note) => {
                    self.state = ParserState::NoteOnRecvd(channel);
                    Some(MidiMessage::NoteOn(channel, note, byte.into()))
                }

                ParserState::KeyPressureRecvd(channel) => {
                    self.state = ParserState::KeyPressureNoteRecvd(channel, byte.into());
                    None
                }
                ParserState::KeyPressureNoteRecvd(channel, note) => {
                    self.state = ParserState::KeyPressureRecvd(channel);
                    Some(MidiMessage::KeyPressure(channel, note, byte.into()))
                }

                ParserState::ControlChangeRecvd(channel) => {
                    self.state = ParserState::ControlChangeControlRecvd(channel, byte.into());
                    None
                }
                ParserState::ControlChangeControlRecvd(channel, control) => {
                    self.state = ParserState::ControlChangeRecvd(channel);
                    Some(MidiMessage::ControlChange(channel, control, byte.into()))
                }

                ParserState::ProgramChangeRecvd(channel) => {
                    Some(MidiMessage::ProgramChange(channel, byte.into()))
                }

                ParserState::ChannelPressureRecvd(channel) => {
                    Some(MidiMessage::ChannelPressure(channel, byte.into()))
                }

                ParserState::PitchBendRecvd(channel) => {
                    self.state = ParserState::PitchBendFirstByteRecvd(channel, byte);
                    None
                }
                ParserState::PitchBendFirstByteRecvd(channel, byte1) => {
                    self.state = ParserState::PitchBendRecvd(channel);
                    Some(MidiMessage::PitchBendChange(channel, (byte1, byte).into()))
                }
                ParserState::QuarterFrameRecvd => Some(MidiMessage::QuarterFrame(byte.into())),
                ParserState::SongPositionRecvd => {
                    self.state = ParserState::SongPositionLsbRecvd(byte);
                    None
                }
                ParserState::SongPositionLsbRecvd(lsb) => {
                    self.state = ParserState::SongPositionRecvd;
                    Some(MidiMessage::SongPositionPointer((lsb, byte).into()))
                }
                ParserState::SongSelectRecvd => Some(MidiMessage::SongSelect(byte.into())),
                _ => None,
            }
        }
    }
}

