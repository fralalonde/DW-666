use core::convert::{TryFrom, TryInto};
use Message::*;
use CodeIndexNumber::{SystemCommonLen1, SystemCommonLen2, SystemCommonLen3};
use crate::{Channel, Note, Velocity, Pressure, Program, Control, U7, Bend, CodeIndexNumber, Packet, Status, MidiError, Cull};
use crate::status::{SYSEX_END, is_non_status, SYSEX_START};

#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
    SysexEmpty,
    SysexSingleByte(u8),
}

pub fn note_on(channel: Channel, note: impl TryInto<Note>, velocity: impl TryInto<Velocity>) -> Result<Message, MidiError> {
    Ok(NoteOn(
        channel,
        note.try_into().map_err(|_| MidiError::InvalidNote)?,
        velocity.try_into().map_err(|_| MidiError::InvalidVelocity)?)
    )
}

pub fn note_off(channel: Channel, note: impl TryInto<Note>, velocity: impl TryInto<Velocity>) -> Result<Message, MidiError> {
    Ok(NoteOff(
        channel,
        note.try_into().map_err(|_| MidiError::InvalidNote)?,
        velocity.try_into().map_err(|_| MidiError::InvalidVelocity)?)
    )
}

pub fn program_change(channel: Channel, program: impl TryInto<Program>) -> Result<Message, MidiError> {
    Ok(ProgramChange(
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

            (..) => Err(MidiError::BadPacket(packet)),
        }
    }
}
