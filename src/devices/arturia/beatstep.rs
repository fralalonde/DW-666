//! From https://www.untergeek.de/2014/11/taming-arturias-beatstep-sysex-codes-for-programming-via-ipad/
//! Thanks to Richard Wanderlöf and Untergeek
//! Switching the LEDs on and off:

use crate::midi::{U7, U4, Note, Program, Control, Channel, Cull, MidiError, Sysex, Matcher, Token, Tag};
use alloc::vec::Vec;

use Token::{Seq, Cap, Val, Buf};
use Tag::*;

const ID_FORMAT: u8 = 0x40;
const DATA_FORMAT: u8 = 0x30;

const WRITE_OK: u8 = 0x21;
const WRITE_ERR: u8 = 0x22;

const ARTURIA: &'static [u8] = &[0x00, 0x20];
const BEATSTEP: &'static [u8] = &[0x6B, 0x7F];

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

pub fn parameter_set(param: u8, control: u8, value: u8) -> Sysex {
    Sysex::new(vec![Seq(ARTURIA), Seq(BEATSTEP), Seq(&[0x42, 0x02, 0x00]), Val(param), Val(control), Val(value)])
}

const MODE: u8 = 0x01;

pub fn beatstep_set(param: Param) -> Vec<Sysex> {
    match param {
        Param::PadOff(pad) =>
            vec![parameter_set(MODE, pad.control_code(), PadMode::PadOff as u8)],
        Param::PadMMC(pad, mmc) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::PadMMC as u8),
                parameter_set(0x03, ccode, mmc as u8)
            ]
        }
        Param::PadCC(pad, channel, cc, on, off, switch) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::PadCC as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, cc.0),
                parameter_set(0x04, ccode, on as u8),
                parameter_set(0x05, ccode, off as u8),
                parameter_set(0x06, ccode, switch as u8),
            ]
        }
        Param::PadCCSilent(pad, channel, cc, on, off, switch) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::PadCCSilent as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, cc.0),
                parameter_set(0x04, ccode, on as u8),
                parameter_set(0x05, ccode, off as u8),
                parameter_set(0x06, ccode, switch as u8),
            ]
        }
        Param::PadNote(pad, channel, note, switch) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::PadNote as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, note.0),
                parameter_set(0x06, ccode, switch as u8),
            ]
        }
        Param::PadProgramChange(pad, channel, program, lsb, msb) => {
            let ccode = pad.control_code();
            vec![
                parameter_set(MODE, ccode, PadMode::PadProgramChange as u8),
                parameter_set(0x02, ccode, channel.0),
                parameter_set(0x03, ccode, program.0),
                parameter_set(0x04, ccode, lsb.0),
                parameter_set(0x05, ccode, msb.0),
            ]
        }
        Param::KnobOff(_) => {}
        Param::KnobCC(_, _, _, _, _, _) => {}
        Param::KnobNRPN(_, _, _, _, _, _) => {}
        Param::GlobalMidiChannel(_) => {}
        Param::CVGateChannel(_) => {}
        Param::KnobAcceleration(_) => {}
        Param::PadVelocityCurve(_) => {}
        Param::StepNote(_, _, _) => {}
        Param::StepEnabled(_, _, _) => {}
    }
}

pub fn parameter_get(param: u8, control: u8) -> Sysex {
    Sysex::new(vec![Seq(ARTURIA), Seq(BEATSTEP), Seq(&[0x42, 0x01, 0x00]), Val(param), Val(control)])
}

pub fn parameter_match() -> Matcher {
    Matcher::new(vec![Seq(ARTURIA), Seq(BEATSTEP), Cap(ValueU7), Cap(ValueU7), Cap(ValueU7)])
}

pub fn write_matcher() -> Matcher {
    Matcher::new(vec![Seq(DATA_HEADER), Cap(ValueU7)])
}

pub fn dump_request() -> Sysex {
    Sysex::new(vec![Seq(DATA_HEADER), Val(0x10)])
}

pub fn dump_matcher() -> Matcher {
    Matcher::new(vec![Seq(DATA_HEADER), Val(0x40), Cap(Dump(26))])
}


// const GET_CTL_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x01, 0x00, /* param, control */];
//
// const SET_CTL_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, /* param, control, value */];
//
// const GET_SEQ_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x01, 0x00, /* param, step */];
//
// const SET_SEQ_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, /* param, step, value */];


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

pub type PadNum = U4;

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
            Pad::Pad(num) => 0x70 + *num,
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
            Encoder::Knob(num) => 0x20 + *num,
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
    PadOff = 0,
    PadMMC = 7,
    PadCC = 8,
    PadCCSilent = 1,
    PadNote = 9,
    PadProgramChange = 0x0B,
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

    StepNote(Channel, StepNum, Note),
    StepEnabled(Channel, StepNum, bool),

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

            _ => Err(MidiError::NoModeForParameter)?
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



