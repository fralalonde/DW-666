use crate::midi::u7::U7;
use crate::midi::{MidiError};
use core::convert::{TryFrom, TryInto};
use crate::midi::status::MidiStatus::{ChannelStatus, SystemStatus};
use crate::midi::u4::U4;

use num_enum::TryFromPrimitive;
use crate::midi::notes::Note;
use crate::midi::u14::U14;
use crate::midi::packet::MidiPacket;

pub type Channel = U4;
pub type Velocity = U7;
pub type Control = U7;
pub type Pressure = U7;
pub type Patch = U7;
pub type Bend = U14;

/// Excluding Sysex
pub enum MidiMessage {
    NoteOff(Channel, Note, Velocity),
    NoteOn(Channel, Note, Velocity),

    NotePressure(Channel, Note, Pressure),
    ChannelPressure(Channel, Pressure),
    ProgramChange(Channel, Patch),
    ControlChange(Channel, Control, U7),
    PitchBend(Channel, Bend),

    // System
    TimeCodeQuarterFrame(U7),
    SongPositionPointer(U7, U7),
    SongSelect(U7),
    TuneRequest,

    // System Realtime
    TimingClock,
    // MeasureEnd, unused
    Start,
    Continue,
    Stop,
    ActiveSensing,
    SystemReset,
}

// impl MidiMessage {
//     pub fn note_off<C, N, V, E>(channel: C, note: N, velocity: V) -> Result<Self, E>
//         where C: TryInto<Channel, Error=E>,
//               N: TryInto<Note, Error=E>,
//               V: TryInto<Velocity, Error=E>
//     {
//         Ok(NoteOff(channel.try_into()?, note.try_into()?, velocity.try_into()?))
//     }
// }

impl TryFrom<MidiPacket> for MidiMessage {
    type Error = MidiError;

    fn try_from(_packet: MidiPacket) -> Result<Self, Self::Error> {
        unimplemented!()
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
        Parser::new().assert_result(&[0x82, 0x76, 0x34], &[MidiMessage::NoteOff(
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
                MidiMessage::NoteOff(2.into(), 0x76.into(), 0x34.into()),
                MidiMessage::NoteOff(2.into(), 0x33.into(), 0x65.into()),
            ],
        );
    }

    #[test]
    fn should_parse_note_on() {
        Parser::new().assert_result(&[0x91, 0x04, 0x34], &[MidiMessage::NoteOn(
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
                MidiMessage::NoteOn(2.into(), 0x76.into(), 0x34.into()),
                MidiMessage::NoteOn(2.into(), 0x33.into(), 0x65.into()),
            ],
        );
    }

    #[test]
    fn should_parse_keypressure() {
        Parser::new().assert_result(&[0xAA, 0x13, 0x34], &[MidiMessage::KeyPressure(
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
                MidiMessage::KeyPressure(8.into(), 0x77.into(), 0x03.into()),
                MidiMessage::KeyPressure(8.into(), 0x14.into(), 0x56.into()),
            ],
        );
    }

    #[test]
    fn should_parse_control_change() {
        Parser::new().assert_result(&[0xB2, 0x76, 0x34], &[MidiMessage::ControlChange(
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
                MidiMessage::ControlChange(3.into(), 0x3c.into(), 0x18.into()),
                MidiMessage::ControlChange(3.into(), 0x43.into(), 0x01.into()),
            ],
        );
    }

    #[test]
    fn should_parse_program_change() {
        Parser::new().assert_result(&[0xC9, 0x15], &[MidiMessage::ProgramChange(
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
                MidiMessage::ProgramChange(3.into(), 0x67.into()),
                MidiMessage::ProgramChange(3.into(), 0x01.into()),
            ],
        );
    }

    #[test]
    fn should_parse_channel_pressure() {
        Parser::new().assert_result(&[0xDD, 0x37], &[MidiMessage::ChannelPressure(
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
                MidiMessage::ChannelPressure(6.into(), 0x77.into()),
                MidiMessage::ChannelPressure(6.into(), 0x43.into()),
            ],
        );
    }

    #[test]
    fn should_parse_pitchbend() {
        Parser::new().assert_result(&[0xE8, 0x14, 0x56], &[MidiMessage::PitchBendChange(
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
                MidiMessage::PitchBendChange(3.into(), (0x3c, 0x18).into()),
                MidiMessage::PitchBendChange(3.into(), (0x43, 0x01).into()),
            ],
        );
    }

    #[test]
    fn should_parse_quarter_frame() {
        Parser::new().assert_result(&[0xf1, 0x7f], &[MidiMessage::QuarterFrame(0x7f.into())]);
    }

    #[test]
    fn should_handle_quarter_frame_running_state() {
        Parser::new().assert_result(
            &[
                0xf1, 0x7f, // Send quarter frame
                0x56, // Only send data of next quarter frame
            ],
            &[
                MidiMessage::QuarterFrame(0x7f.into()),
                MidiMessage::QuarterFrame(0x56.into()),
            ],
        );
    }

    #[test]
    fn should_parse_song_position_pointer() {
        Parser::new().assert_result(&[0xf2, 0x7f, 0x68], &[MidiMessage::SongPositionPointer(
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
                MidiMessage::SongPositionPointer((0x7f, 0x68).into()),
                MidiMessage::SongPositionPointer((0x23, 0x7b).into()),
            ],
        );
    }

    #[test]
    fn should_parse_song_select() {
        Parser::new().assert_result(&[0xf3, 0x3f], &[MidiMessage::SongSelect(0x3f.into())]);
    }

    #[test]
    fn should_handle_song_select_running_state() {
        Parser::new().assert_result(
            &[
                0xf3, 0x3f, // Send song select
                0x00, // Only send data for next song select
            ],
            &[
                MidiMessage::SongSelect(0x3f.into()),
                MidiMessage::SongSelect(0x00.into()),
            ],
        );
    }

    #[test]
    fn should_parse_tune_request() {
        Parser::new().assert_result(&[0xf6], &[MidiMessage::TuneRequest]);
    }

    #[test]
    fn should_interrupt_parsing_for_tune_request() {
        Parser::new().assert_result(
            &[
                0x92, 0x76, // start note_on message
                0xf6, // interrupt with tune request
                0x34, // finish note on, this should be ignored
            ],
            &[MidiMessage::TuneRequest],
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
        Parser::new().assert_result(&[0xf8], &[MidiMessage::TimingClock]);
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
                MidiMessage::TimingClock,
                MidiMessage::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_start_message() {
        Parser::new().assert_result(&[0xfa], &[MidiMessage::Start]);
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
                MidiMessage::Start,
                MidiMessage::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_continue_message() {
        Parser::new().assert_result(&[0xfb], &[MidiMessage::Continue]);
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
                MidiMessage::Continue,
                MidiMessage::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_stop_message() {
        Parser::new().assert_result(&[0xfc], &[MidiMessage::Stop]);
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
                MidiMessage::Stop,
                MidiMessage::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_activesensing_message() {
        Parser::new().assert_result(&[0xfe], &[MidiMessage::ActiveSensing]);
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
                MidiMessage::ActiveSensing,
                MidiMessage::ChannelPressure(6.into(), 0x77.into()),
            ],
        );
    }

    #[test]
    fn should_parse_reset_message() {
        Parser::new().assert_result(&[0xff], &[MidiMessage::Reset]);
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
                MidiMessage::Reset,
                MidiMessage::ChannelPressure(6.into(), 0x77.into()),
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
            &[MidiMessage::NoteOff(2.into(), 0x76.into(), 0x34.into())],
        );
    }

    impl Parser {
        /// Test helper function, asserts if a slice of bytes parses to some set of midi events
        fn assert_result(&mut self, bytes: &[u8], expected_events: &[MidiMessage]) {
            let events: Vec<MidiMessage> = bytes
                .into_iter()
                .filter_map(|byte| self.advance(*byte))
                .collect();

            assert_eq!(expected_events, events.as_slice());
        }
    }
}
