//! From https://www.untergeek.de/2014/11/taming-arturias-beatstep-sysex-codes-for-programming-via-ipad/
//! Thanks to Richard Wanderlöf and Untergeek
//! Switching the LEDs on and off:

#![allow(unused)]
#![allow(clippy::upper_case_acronyms)]
use midi::{U7, U4, Note, Program, Control, Channel, MidiError};
use alloc::vec::Vec;

use crate::sysex::Token::{Seq, Cap, Val};
use crate::sysex::Tag::*;
use crate::sysex::Sysex;
use crate::sysex;

const ID_FORMAT: u8 = 0x40;
const DATA_FORMAT: u8 = 0x30;

const WRITE_OK: u8 = 0x21;
const WRITE_ERR: u8 = 0x22;

const ARTURIA: &[u8] = &[0x00, 0x20];
const BEATSTEP: &[u8] = &[0x6B, 0x7F];

// pub fn id_request() -> Sysex {
//     Sysex::new(vec![Seq(ID_HEADER)])
// }
//
// pub fn id_matcher() -> Matcher {
//     Matcher::new(vec![Seq(ID_HEADER), Val(DW_6000)])
// }
//
// pub fn write(program: u8) -> Sysex {
//     Sysex::new(vec![Seq(DATA_HEADER), Val(0x11), Val(program)])
// }
//
// pub fn load(dump: Vec<u8>) -> Sysex {
//     Sysex::new(vec![Seq(DATA_HEADER), Buf(dump)])
// }

fn parameter_set(param: u8, control: u8, value: u8) -> Sysex {
    Sysex::new(vec![Seq(ARTURIA), Seq(BEATSTEP), Seq(&[0x42, 0x02, 0x00]), Val(param), Val(control), Val(value)])
}

const MODE: u8 = 0x01;
const MIDI_CHANNEL: u8 = 0x50;
const CURVE: u8 = 0x41;
const STEP_NOTE: u8 = 0x52;
const STEP_ENABLED: u8 = 0x53;
const SEQ: u8 = 0x50;

pub fn beatstep_set(param: Param) -> Vec<Sysex> {
    match param {
        Param::PadOff(pad) =>
            vec![parameter_set(MODE, pad.control_code(), PadMode::Off as u8)],
        Param::PadMMC(pad, mmc) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::MMC as u8),
                parameter_set(0x03, ccode, mmc as u8)
            ]
        }
        Param::PadCC(pad, channel, cc, on, off, switch) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::CC as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, cc.0),
                parameter_set(0x04, ccode, on.0),
                parameter_set(0x05, ccode, off.0),
                parameter_set(0x06, ccode, switch as u8),
            ]
        }
        Param::PadCCSilent(pad, channel, cc, on, off, switch) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::CCSilent as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, cc.0),
                parameter_set(0x04, ccode, on.0),
                parameter_set(0x05, ccode, off.0),
                parameter_set(0x06, ccode, switch as u8),
            ]
        }
        Param::PadNote(pad, channel, note, switch) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::Note as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, note as u8),
                parameter_set(0x06, ccode, switch as u8),
            ]
        }
        Param::PadProgramChange(pad, channel, program, lsb, msb) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::ProgramChange as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, program.0),
                parameter_set(0x04, ccode, lsb.0),
                parameter_set(0x05, ccode, msb.0),
            ]
        }
        Param::KnobOff(knob) => {
            let ccode = knob.control_code();
            vec![
                parameter_set(MODE, ccode, KnobMode::Off as u8),
            ]
        }
        Param::KnobCC(encoder, channel, control, minimum, maximum, behavior) => {
            let ccode = encoder.control_code();
            vec![
                parameter_set(MODE, ccode, KnobMode::CC as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, control.0),
                parameter_set(0x04, ccode, minimum.0),
                parameter_set(0x05, ccode, maximum.0),
                parameter_set(0x06, ccode, behavior as u8),
            ]
        }
        Param::KnobNRPN(knob, channel, granularity, banklsb, bankmsb, nrpntype) => {
            let ccode = knob.control_code();
            vec![
                parameter_set(MODE, ccode, KnobMode::NRPN as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, granularity as u8),
                parameter_set(0x04, ccode, banklsb.0),
                parameter_set(0x05, ccode, bankmsb.0),
                parameter_set(0x06, ccode, nrpntype as u8),
            ]
        }

        Param::GlobalMidiChannel(channel) =>
            vec![parameter_set(MIDI_CHANNEL, 0x0B, channel.0)],
        Param::CVGateChannel(channel) =>
            vec![parameter_set(MIDI_CHANNEL, 0x0C, channel.0)],
        Param::KnobAcceleration(acceleration) =>
            vec![parameter_set(CURVE, 0x04, acceleration as u8)],
        Param::PadVelocityCurve(vel_curve) =>
            vec![parameter_set(CURVE, 0x03, vel_curve as u8)],

        Param::StepNote(stepnum, note) =>
            vec![parameter_set(STEP_NOTE, stepnum.0, note as u8)],
        Param::StepEnabled(stepnum, bool) =>
            vec![parameter_set(STEP_ENABLED, stepnum.0, if bool { 1 } else { 0 })],
        Param::SeqChannel(channel) =>
            vec![parameter_set(SEQ, SeqGlobal::Channel as u8, channel.0)],
        Param::SeqTranspose(root_note) =>
            vec![parameter_set(SEQ, SeqGlobal::Transpose as u8, root_note.0 as u8)],
        Param::SeqScale(scale) =>
            vec![parameter_set(SEQ, SeqGlobal::Scale as u8, scale as u8)],
        Param::SeqMode(mode) =>
            vec![parameter_set(SEQ, SeqGlobal::Mode as u8, mode as u8)],
        Param::SeqStepSize(size) =>
            vec![parameter_set(SEQ, SeqGlobal::StepSize as u8, size as u8)],
        Param::SeqPatternLength(plen) =>
            vec![parameter_set(SEQ, SeqGlobal::PatternLength as u8, plen.0)],
        Param::SeqSwing(value) =>
            vec![parameter_set(SEQ, SeqGlobal::Swing as u8, value.0)],
        Param::SeqGate(value) =>
            vec![parameter_set(SEQ, SeqGlobal::Gate as u8, value.0)],
        Param::SeqLegato(value) =>
            vec![parameter_set(SEQ, SeqGlobal::Legato as u8, value as u8)],
    }
}

pub fn beatstep_control_get(param: u8, control: u8) -> Sysex {
    Sysex::new(vec![Seq(ARTURIA), Seq(BEATSTEP), Seq(&[0x42, 0x01, 0x00]), Val(param), Val(control)])
}

pub fn parameter_match() -> sysex::Matcher {
    sysex::Matcher::new(vec![Seq(ARTURIA), Seq(BEATSTEP), Cap(ValueU7), Cap(ValueU7), Cap(ValueU7)])
}

#[derive(Debug)]
#[repr(u8)]
pub enum MMC {
    Stop = 1,
    Play = 2,
    DeferredPlay = 3,
    FastForward = 4,
    Rewind = 5,
    RecordStrobe = 6,
    RecordExit = 7,
    RecordReady = 8,
    Pause = 9,
    Eject = 10,
    Chase = 11,
    InListReset = 12,
}

pub type PadNum = u8;

#[derive(Debug)]
pub enum Pad {
    Pad(PadNum),
    Start,
    Stop,
    CtrlSeq,
    ExtSync,
    Recall,
    Store,
    Shift,
    Chan,
}

trait ControlCode {
    fn control_code(&self) -> u8;
}

impl ControlCode for Pad {
    fn control_code(&self) -> u8 {
        match self {
            Pad::Pad(num) => 0x70 + num,
            Pad::Start => 0x70,
            Pad::Stop => 0x58,
            Pad::CtrlSeq => 0x5A,
            Pad::ExtSync => 0x5B,
            Pad::Recall => 0x5C,
            Pad::Store => 0x5D,
            Pad::Shift => 0x5E,
            Pad::Chan => 0x5F,
        }
    }
}

impl ControlCode for Encoder {
    fn control_code(&self) -> u8 {
        match self {
            Encoder::Knob(num) => 0x20 + num.0,
            Encoder::JogWheel => 0x30,
        }
    }
}

pub type OnValue = U7;
pub type OffValue = U7;

#[derive(Debug)]
#[repr(u8)]
pub enum SwitchMode {
    Toggle = 0,
    Gate = 1,
}

pub type BankLSB = U7;
pub type BankMSB = U7;
pub type StepNum = U4;

enum PadMode {
    Off = 0,
    MMC = 7,
    CC = 8,
    CCSilent = 1,
    Note = 9,
    ProgramChange = 0x0B,
}

enum KnobMode {
    Off = 0,
    CC = 1,
    NRPN = 4,
}

/// base note is C5= 0x3C, to transpose down 12 semitones to C4, nn=0x30 and so on
#[derive(Debug)]
pub struct SeqTranspose(Note);

#[derive(Debug)]
#[repr(u8)]
pub enum SeqScale {
    Chromatic,
    Major,
    Minor,
    Dorian,
    Mixolydian,
    HarmonicMinor,
    Blues,
    User,
}

#[derive(Debug)]
#[repr(u8)]
pub enum SeqMode {
    Forward,
    Reverse,
    Alternating,
    Random,
}

#[derive(Debug)]
#[repr(u8)]
pub enum SeqStepSize {
    Quarter,
    Eight,
    Sixteenth,
    ThirtyTwat,
}

#[derive(Debug)]
pub struct SeqPatternLength(u8);

#[derive(Debug)]
pub struct SeqSwing(u8);

#[derive(Debug)]
pub struct SeqGateTime(u8);

#[derive(Debug)]
#[repr(u8)]
pub enum SeqLegato {
    Off,
    On,
    Reset,
}

/// Pressure-sensitive pad config
#[derive(Debug)]
pub enum Param {
    PadOff(Pad),
    PadMMC(Pad, MMC),
    PadCC(Pad, Channel, Control, OnValue, OffValue, SwitchMode),
    PadCCSilent(Pad, Channel, Control, OnValue, OffValue, SwitchMode),
    PadNote(Pad, Channel, Note, SwitchMode),
    PadProgramChange(Pad, Channel, Program, BankLSB, BankMSB),

    KnobOff(Encoder),
    KnobCC(Encoder, Channel, Control, Minimum, Maximum, Behavior),
    KnobNRPN(Encoder, Channel, Granularity, BankLSB, BankMSB, NRPNType),

    GlobalMidiChannel(Channel),
    CVGateChannel(Channel),
    KnobAcceleration(Acceleration),
    PadVelocityCurve(VelocityCurve),

    StepNote(StepNum, Note),
    StepEnabled(StepNum, bool),
    SeqChannel(Channel),
    SeqTranspose(SeqTranspose),
    SeqScale(SeqScale),
    SeqMode(SeqMode),
    SeqStepSize(SeqStepSize),
    SeqPatternLength(SeqPatternLength),
    SeqSwing(SeqSwing),
    SeqGate(SeqGateTime),
    SeqLegato(SeqLegato),
}


#[derive(Debug)]
#[repr(u8)]
pub enum PadSwitchParam {
    Channel = 2,
    Control = 3,
    Off = 4,
    On = 5,
    SwitchMode = 6,
}

#[derive(Debug)]
#[repr(u8)]
pub enum PadNoteParam {
    Channel = 2,
    Note = 3,
    SwitchMode = 6,
}

#[derive(Debug)]
#[repr(u8)]
pub enum PadProgramParam {
    Channel = 2,
    Program = 3,
    BankLSB = 4,
    BankMSB = 5,
}

impl Param {
    fn mode_code(&self) -> Result<u8, MidiError> {
        Ok(match self {
            Param::PadOff(_) | Param::KnobOff(_) => 0,
            Param::PadMMC(..) => 7,
            Param::PadCC(..) => 8,
            Param::PadCCSilent(..) | Param::KnobCC(..) => 1,
            Param::PadNote(..) => 9,
            Param::PadProgramChange(..) => 0xB,

            Param::KnobOff(_) => 0,
            Param::KnobNRPN(..) => 4,

            _ => return Err(MidiError::NoModeForParameter)
        })
    }
}

/// Rotary Encoder config
pub type KnobNum = U4;

#[derive(Debug)]
pub enum Encoder {
    Knob(KnobNum),
    JogWheel,
}

#[derive(Debug)]
#[repr(u8)]
pub enum EncoderControlParam {
    Channel = 2,
    Control = 3,
    Minimum = 4,
    Maximum = 5,
    Behavior = 6,
}

#[derive(Debug)]
#[repr(u8)]
pub enum EncoderNRPNParam {
    Channel = 2,
    Granularity = 3,
    Minimum = 4,
    Maximum = 5,
    Behavior = 6,
}


pub type Minimum = U7;
pub type Maximum = U7;

#[derive(Debug)]
#[repr(u8)]
pub enum Behavior {
    Absolute = 0,
    RelativeCentered64 = 1,
    RelativeCentered0 = 2,
    RelativeCentered16 = 3,
}

#[derive(Debug)]
#[repr(u8)]
pub enum Granularity {
    /// Controls MSB
    Coarse = 0x06,
    /// Controls LSB
    Fine = 0x26,
}

#[derive(Debug)]
#[repr(u8)]
pub enum NRPNType {
    /// Controls MSB
    NRPN = 0,
    /// Controls LSB
    RPN = 1,
}


#[derive(Debug)]
#[repr(u8)]
pub enum Acceleration {
    Slow = 0,
    Medium = 1,
    Fast = 2,
}

#[derive(Debug)]
#[repr(u8)]
pub enum VelocityCurve {
    Linear = 0,
    Logarithmic = 1,
    Exponential = 2,
    Full = 3,
}

#[derive(Debug)]
#[repr(u8)]
pub enum SeqGlobal {
    Channel = 1,
    Transpose = 2,
    Scale = 3,
    Mode = 4,
    StepSize = 5,
    PatternLength = 6,
    Swing = 7,
    Gate = 8,
    Legato = 9,
}


