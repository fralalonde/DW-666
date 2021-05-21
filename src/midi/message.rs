use crate::midi::u7::U7;

use crate::midi::note::Note;
use crate::midi::packet::{Packet, CodeIndexNumber};
use core::convert::{TryFrom, TryInto};
use crate::midi::{MidiError, Cull, Channel, Velocity, Pressure, Control, Bend, Program};
use crate::midi::status::{Status, SYSEX_END, is_non_status, SYSEX_START};
use Message::*;
use CodeIndexNumber::{SystemCommonLen1, SystemCommonLen2, SystemCommonLen3};

/// Excluding Sysex
#[derive(Clone, Copy, Debug)]
#[allow(unused)]
pub enum Message {
    NoteOff(Channel, Note, Velocity),
    NoteOn(Channel, Note, Velocity),

    NotePressure(Channel, Note, Pressure),
    ChannelPressure(Channel, Pressure),
    ProgramChange(Channel, Program),
    ControlChange(Channel, Control, U7),
    PitchBend(Channel, Bend),

    // System
    TimeCodeQuarterFrame(U7),
    SongPositionPointer(U7, U7),
    SongSelect(U7),
    TuneRequest,

    // System Realtime
    TimingClock,
    MeasureEnd(U7),
    Start,
    Continue,
    Stop,
    ActiveSensing,
    SystemReset,

    // Sysex
    SysexBegin(u8, u8),
    SysexCont(u8, u8, u8),
    SysexEnd,
    SysexEnd1(u8),
    SysexEnd2(u8, u8),

    // "special cases" - as per the USB MIDI spec
    // Begin & End
    SysexEmpty,
    // Begin, Byte & End
    SysexSingleByte(u8),

}

pub fn note_on(channel: Channel, note: impl TryInto<Note>, velocity: impl TryInto<Velocity>) -> Result<Message, MidiError> {
    Ok(Message::NoteOn(
        channel,
        note.try_into().map_err(|_| MidiError::InvalidNote)?,
        velocity.try_into().map_err(|_| MidiError::InvalidVelocity)?)
    )
}

pub fn note_off(channel: Channel, note: impl TryInto<Note>, velocity: impl TryInto<Velocity>) -> Result<Message, MidiError> {
    Ok(Message::NoteOff(
        channel,
        note.try_into().map_err(|_| MidiError::InvalidNote)?,
        velocity.try_into().map_err(|_| MidiError::InvalidVelocity)?)
    )
}

pub fn program_change(channel: Channel, program: impl TryInto<Program>) -> Result<Message, MidiError> {
    Ok(Message::ProgramChange(
        channel,
        program.try_into().map_err(|_| MidiError::InvalidProgram)?,
    ))
}

impl TryFrom<Packet> for Message {
    type Error = MidiError;

    fn try_from(packet: Packet) -> Result<Self, Self::Error> {
        match (packet.code_index_number(), packet.status(), packet.channel(), packet.payload()) {
            (CodeIndexNumber::Sysex, _, _, payload) => {
                if is_non_status(payload[0]) {
                    Ok(SysexCont(payload[0], payload[1], payload[2]))
                } else {
                    Ok(SysexBegin(payload[1], payload[2]))
                }
            }
            (SystemCommonLen1, _, _, payload) if payload[0] == SYSEX_END => Ok(SysexEnd),
            (CodeIndexNumber::SysexEndsNext2, _, _, payload) => {
                if payload[0] == SYSEX_START {
                    Ok(SysexEmpty)
                } else {
                    Ok(SysexEnd1(payload[0]))
                }
            },
            (CodeIndexNumber::SysexEndsNext3, _, _, payload) => {
                if payload[0] == SYSEX_START {
                    Ok(SysexSingleByte(payload[1]))
                } else {
                    Ok(SysexEnd2(payload[0], payload[1]))
                }
            },

            (SystemCommonLen1, Some(Status::TimingClock), ..) => Ok(TimingClock),
            (SystemCommonLen1, Some(Status::TuneRequest), ..) => Ok(TuneRequest),
            (SystemCommonLen1, Some(Status::Start), ..) => Ok(Start),
            (SystemCommonLen1, Some(Status::Continue), ..) => Ok(Continue),
            (SystemCommonLen1, Some(Status::Stop), ..) => Ok(Stop),
            (SystemCommonLen1, Some(Status::ActiveSensing), ..) => Ok(ActiveSensing),
            (SystemCommonLen1, Some(Status::SystemReset), ..) => Ok(SystemReset),
            (SystemCommonLen2, Some(Status::TimeCodeQuarterFrame), _, payload) => Ok(TimeCodeQuarterFrame(U7::cull(payload[1]))),
            (SystemCommonLen2, Some(Status::SongSelect), _, payload) => Ok(SongSelect(U7::cull(payload[1]))),
            (SystemCommonLen2, Some(Status::MeasureEnd), _, payload) => Ok(MeasureEnd(U7::cull(payload[1]))),
            (SystemCommonLen3, Some(Status::SystemReset), _, payload) => Ok(SongPositionPointer(U7::cull(payload[1]), U7::cull(payload[1]))),

            (_, Some(Status::NoteOff), Some(channel), payload) => Ok(NoteOff(channel, Note::try_from(payload[1])?, Velocity::try_from(payload[2])?)),
            (_, Some(Status::NoteOn), Some(channel), payload) => Ok(NoteOn(channel, Note::try_from(payload[1])?, Velocity::try_from(payload[2])?)),
            (_, Some(Status::NotePressure), Some(channel), payload) => Ok(NotePressure(channel, Note::try_from(payload[1])?, Pressure::try_from(payload[2])?)),
            (_, Some(Status::ChannelPressure), Some(channel), payload) => Ok(ChannelPressure(channel, Pressure::try_from(payload[1])?)),
            (_, Some(Status::ProgramChange), Some(channel), payload) => Ok(ProgramChange(channel, U7::try_from(payload[1])?)),
            (_, Some(Status::ControlChange), Some(channel), payload) => Ok(ControlChange(channel, Control::try_from(payload[1])?, U7::try_from(payload[2])?)),
            (_, Some(Status::PitchBend), Some(channel), payload) => Ok(PitchBend(channel, Bend::try_from((payload[1], payload[2]))?)),

            (..) => Err(MidiError::UnparseablePacket(packet)),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec::Vec;

    #[test]
    fn should_parse_status_byte() {
        assert!(is_status_byte(0x80u8));
        assert!(is_status_byte(0x94u8));
        assert!(!is_status_byte(0x00u8));
        assert!(!is_status_byte(0x78u8));
    }

    #[test]
    fn should_parse_system_message() {
        assert!(is_system_message(0xf0));
        assert!(is_system_message(0xf4));
        assert!(!is_system_message(0x0f));
        assert!(!is_system_message(0x77));
    }

    #[test]
    fn should_split_message_and_channel() {
        let (message, channel) = split_message_and_channel(0x91u8);
        assert_eq!(message, 0x90u8);
        assert_eq!(channel, 1.into());
    }

    #[test]
    fn should_parse_note_off() {
        Parser::new().assert_result(&[0x82, 0x76, 0x34], &[Message::NoteOff(
            2.into(),
            0x76.into(),
            0x34.into(),
        )]);
    }

    #[test]
    fn should_handle_note_off_running_state() {
        Parser::new().assert_result(
            &[
                0x82, 0x76, 0x34, // First note_off
                0x33, 0x65, // Second note_off without status byte
            ],
            &[
                Message::NoteOff(2.into(), 0x76.into(), 0x34.into()),
                Message::NoteOff(2.into(), 0x33.into(), 0x65.into()),
            ],
        );
    }

    #[test]
    fn should_parse_note_on() {
        Parser::new().assert_result(&[0x91, 0x04, 0x34], &[Message::NoteOn(
            1.into(),
            4.into(),
            0x34.into(),
        )]);
    }

    #[test]
    fn should_handle_note_on_running_state() {
        Parser::new().assert_result(
            &[
                0x92, 0x76, 0x34, // First note_on
                0x33, 0x65, // Second note on without status byte
            ],
            &[
                Message::NoteOn(2.into(), 0x76.into(), 0x34.into()),
                Message::NoteOn(2.into(), 0x33.into(), 0x65.into()),
            ],
        );
    }

    #[test]
    fn should_parse_keypressure() {
        Parser::new().assert_result(&[0xAA, 0x13, 0x34], &[Message::KeyPressure(
            10.into(),
            0x13.into(),
            0x34.into(),
        )]);
    }

    #[test]
    fn should_handle_keypressure_running_state() {
        Parser::new().assert_result(
            &[
                0xA8, 0x77, 0x03, // First key_pressure
                0x14, 0x56, // Second key_pressure without status byte
            ],
            &[
                Message::KeyPressure(8.into(), 0x77.into(), 0x03.into()),
                Message::KeyPressure(8.into(), 0x14.into(), 0x56.into()),
            ],
        );
    }

    #[test]
    fn should_parse_control_change() {
        Parser::new().assert_result(&[0xB2, 0x76, 0x34], &[Message::ControlChange(
            2.into(),
            0x76.into(),
            0x34.into(),
        )]);
    }

    #[test]
    fn should_parse_control_change_running_state() {
        Parser::new().assert_result(
            &[
                0xb3, 0x3C, 0x18, // First control change
                0x43, 0x01, // Second control change without status byte
            ],
            &[
                Message::ControlChange(3.into(), 0x3c.into(), 0x18.into()),
                Message::ControlChange(3.into(), 0x43.into(), 0x01.into()),
            ],
        );
    }

    #[test]
    fn should_parse_program_change() {
        Parser::new().assert_result(&[0xC9, 0x15], &[Message::ProgramChange(
            9.into(),
            0x15.into(),
        )]);
    }

    #[test]
    fn should_parse_program_change_running_state() {
        Parser::new().assert_result(
            &[
                0xC3, 0x67, // First program change
                0x01, // Second program change without status byte
            ],
            &[
                Message::ProgramChange(3.into(), 0x67.into()),
                Message::ProgramChange(3.into(), 0x01.into()),
            ],
        );
    }

    #[test]
    fn should_parse_channel_pressure() {
        Parser::new().assert_result(&[0xDD, 0x37], &[Message::ChannelPressure(
            13.into(),
            0x37.into(),
        )]);
    }

    #[test]
    fn should_parse_channel_pressure_running_state() {
        Parser::new().assert_result(
            &[
                0xD6, 0x77, // First channel pressure
                0x43, // Second channel pressure without status byte
            ],
            &[
                Message::ChannelPressure(6.into(), 0x77.into()),
                Message::ChannelPressure(6.into(), 0x43.into()),
            ],
        );
    }

    #[test]
    fn should_parse_pitchbend() {
        Parser::new().assert_result(&[0xE8, 0x14, 0x56], &[Message::PitchBendChange(
            8.into(),
            (0x14, 0x56).into(),
        )]);
    }

    #[test]
    fn should_parse_pitchbend_running_state() {
        Parser::new().assert_result(
            &[
                0xE3, 0x3C, 0x18, // First pitchbend
                0x43, 0x01, // Second pitchbend without status byte
            ],
            &[
                Message::PitchBendChange(3.into(), (0x3c, 0x18).into()),
                Message::PitchBendChange(3.into(), (0x43, 0x01).into()),
            ],
        );
    }

    #[test]
    fn should_parse_quarter_frame() {
        Parser::new().assert_result(&[0xf1, 0x7f], &[Message::QuarterFrame(0x7f.into())]);
    }

    #[test]
    fn should_handle_quarter_frame_running_state() {
        Parser::new().assert_result(
            &[
                0xf1, 0x7f, // Send quarter frame
                0x56, // Only send data of next quarter frame
            ],
            &[
                Message::QuarterFrame(0x7f.into()),
                Message::QuarterFrame(0x56.into()),
            ],
        );
    }

    #[test]
    fn should_parse_song_position_pointer() {
        Parser::new().assert_result(&[0xf2, 0x7f, 0x68], &[Message::SongPositionPointer(
            (0x7f, 0x68).into(),
        )]);
    }

    #[test]
    fn should_handle_song_position_pointer_running_state() {
        Parser::new().assert_result(
            &[
                0xf2, 0x7f, 0x68, // Send song position pointer
                0x23, 0x7b, // Only send data of next song position pointer
            ],
            &[
                Message::SongPositionPointer((0x7f, 0x68).into()),
                Message::SongPositionPointer((0x23, 0x7b).into()),
            ],
        );
    }

    #[test]
    fn should_parse_song_select() {
        Parser::new().assert_result(&[0xf3, 0x3f], &[Message::SongSelect(0x3f.into())]);
    }

    #[test]
    fn should_handle_song_select_running_state() {
        Parser::new().assert_result(
            &[
                0xf3, 0x3f, // Send song select
                0x00, // Only send data for next song select
            ],
            &[
                Message::SongSelect(0x3f.into()),
                Message::SongSelect(0x00.into()),
            ],
        );
    }

    #[test]
    fn should_parse_tune_request() {
        Parser::new().assert_result(&[0xf6], &[Message::TuneRequest]);
    }

    #[test]
    fn should_interrupt_parsing_for_tune_request() {
        Parser::new().assert_result(
            &[
                0x92, 0x76, // start note_on message
                0xf6, // interrupt with tune request
                0x34, // finish note on, this should be ignored
            ],
            &[Message::TuneRequest],
        );
    }

    // #[test]
    // fn should_parse_end_exclusive() {
    //     MidiParser::new().assert_result(&[0xf7], &[MidiMessage::EndOfExclusive]);
    // }

    // #[test]
    // fn should_interrupt_parsing_for_end_of_exclusive() {
    //     MidiParser::new().assert_result(
    //         &[
    //             0x92, 0x76, // start note_on message
    //             0xf7, // interrupt with end of exclusive
    //             0x34, // finish note on, this should be ignored
    //         ],
    //         &[MidiMessage::EndOfExclusive],
    //     );
    // }

    #[test]
    fn should_interrupt_parsing_for_undefined_message() {
        Parser::new().assert_result(
            &[
                0x92, 0x76, // start note_on message
                0xf5, // interrupt with undefined message
                0x34, // finish note on, this should be ignored
            ],
            &[],
        );
    }

    #[test]
    fn should_parse_timingclock_message() {
        Parser::new().assert_result(&[0xf8], &[Message::TimingClock]);
    }

    #[test]
    fn should_parse_timingclock_message_as_realtime() {
        Parser::new().assert_result(
            &[
                0xD6, // Start channel pressure event
                0xf8, // interupt with midi timing clock
                0x77, // Finish channel pressure
            ],
            &[
                Message::TimingClock,
                Message::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_start_message() {
        Parser::new().assert_result(&[0xfa], &[Message::Start]);
    }

    #[test]
    fn should_parse_start_message_as_realtime() {
        Parser::new().assert_result(
            &[
                0xD6, // Start channel pressure event
                0xfa, // interupt with start
                0x77, // Finish channel pressure
            ],
            &[
                Message::Start,
                Message::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_continue_message() {
        Parser::new().assert_result(&[0xfb], &[Continue]);
    }

    #[test]
    fn should_parse_continue_message_as_realtime() {
        Parser::new().assert_result(
            &[
                0xD6, // Start channel pressure event
                0xfb, // interupt with continue
                0x77, // Finish channel pressure
            ],
            &[
                Continue,
                Message::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_stop_message() {
        Parser::new().assert_result(&[0xfc], &[Message::Stop]);
    }

    #[test]
    fn should_parse_stop_message_as_realtime() {
        Parser::new().assert_result(
            &[
                0xD6, // Start channel pressure event
                0xfc, // interupt with stop
                0x77, // Finish channel pressure
            ],
            &[
                Message::Stop,
                Message::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_activesensing_message() {
        Parser::new().assert_result(&[0xfe], &[Message::ActiveSensing]);
    }

    #[test]
    fn should_parse_activesensing_message_as_realtime() {
        Parser::new().assert_result(
            &[
                0xD6, // Start channel pressure event
                0xfe, // interupt with activesensing
                0x77, // Finish channel pressure
            ],
            &[
                Message::ActiveSensing,
                Message::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_reset_message() {
        Parser::new().assert_result(&[0xff], &[Message::Reset]);
    }

    #[test]
    fn should_parse_reset_message_as_realtime() {
        Parser::new().assert_result(
            &[
                0xD6, // Start channel pressure event
                0xff, // interupt with reset
                0x77, // Finish channel pressure
            ],
            &[
                Message::Reset,
                Message::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_ignore_incomplete_messages() {
        Parser::new().assert_result(
            &[
                0x92, 0x1b, // Start note off message
                0x82, 0x76, 0x34, // continue with a complete note on message
            ],
            &[Message::NoteOff(2.into(), 0x76.into(), 0x34.into())],
        );
    }

    impl Parser {
        /// Test helper function, asserts if a slice of bytes parses to some set of midi events
        fn assert_result(&mut self, bytes: &[u8], expected_events: &[Message]) {
            let events: Vec<Message> = bytes
                .into_iter()
                .filter_map(|byte| self.advance(*byte))
                .collect();

            assert_eq!(expected_events, events.as_slice());
        }
    }
}
