//! From https://www.untergeek.de/2014/11/taming-arturias-beatstep-sysex-codes-for-programming-via-ipad/
//! Thanks to Richard Wanderlöf and Untergeek
//! Switching the LEDs on and off:

use crate::midi::{U7, U4, Note, Program, Control, Channel, Cull, MidiError,  SysexToken, VarType, SysexMatcher, SysexPackets, Interface, SysexFragment};
use SysexToken::*;
use VarType::*;
use core::convert::TryFrom;
use SysexFragment::{Slice, Byte};


const ARTURIA: &'static [u8] = &[00, 0x20, 0x6B,];
const BEATSTEP: &'static [u8] = &[0x7F, 0x42,];

// Querying current device param
const QUERY: u8 = 0x01;

// Receiving current device param or Sending new value
const VALUE: u8 = 0x02;

// Could become variable when  multiple instances support is required
const DEVICE_ID: u8 = 0x00;

// const QUERY_HEADER: &'static [SysexToken] = &[Match(ARTURIA), Match(BEATSTEP), Match(QUERY), Match(DEVICE_ID), Val(Param) ];
// const VALUE_HEADER: &'static [SysexToken] = &[Match(ARTURIA), Match(BEATSTEP), Match(VALUE), Match(DEVICE_ID) ];

pub fn match_pad() -> SysexMatcher {
    // let mut tlist = TokenList::new();
    // tlist.extend_from_slice()
    SysexMatcher::pattern(&[Match(ARTURIA), Match(BEATSTEP), Val(QUERY), Val(DEVICE_ID), Capture(Index), Capture(Index), Capture(Value)])
}

fn get_pad_mode(param: u8, index: u8) -> Result<PadMode, MidiError> {
    // register matcher on interface
    // send query msg
    // wait for response (timeout) (rust async?)
    // build enum from values
    Ok(PadMode::PadOff(PadNum::try_from(4)?))
}

fn set_pad(param: u8, index: u8, value: u8) -> SysexPackets {
    SysexPackets::sequence(&[Slice(ARTURIA), Slice(BEATSTEP), Byte(QUERY), Byte(DEVICE_ID), Byte(index), Byte(param), Byte(value)])
}

fn match_global() -> SysexMatcher {
    SysexMatcher::pattern(&[Match(ARTURIA), Match(BEATSTEP), Val(QUERY), Val(DEVICE_ID), Capture(Index), Capture(Value)])
}

fn query_global(param: u8) -> SysexPackets {
    SysexPackets::sequence(&[Slice(ARTURIA), Slice(BEATSTEP), Byte(VALUE), Byte(DEVICE_ID), Byte(param)])
}

fn set_global<const TOKENS: usize>(param: u8, value: u8) -> SysexPackets {
    SysexPackets::sequence(&[Slice(ARTURIA), Slice(BEATSTEP), Byte(QUERY), Byte(DEVICE_ID), Byte(param), Byte(value)])
}

const GET_CTL_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x01, 0x00, /* param, control */];

const SET_CTL_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, /* param, control, value */];

const GET_SEQ_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x01, 0x00, /* param, step */];

const SET_SEQ_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, /* param, step, value */];


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
    fn control_code(&self) -> U7;
}

impl ControlCode for Pad {
    fn control_code(&self) -> U7 {
        match self {
            Pad::Pad(num) => U7::cull(0x70 + u8::from(*num)),
            Pad::Start => U7::cull(0x70),
            Pad::Stop => U7::cull(0x58),
            Pad::CtrlSeq => U7::cull(0x5A),
            Pad::ExtSync => U7::cull(0x5B),
            Pad::Recall => U7::cull(0x5C),
            Pad::Store => U7::cull(0x5D),
            Pad::Shift => U7::cull(0x5E),
            Pad::Chan => U7::cull(0x5F),
        }
    }
}

impl ControlCode for Encoder {
    fn control_code(&self) -> U7 {
        match self {
            Encoder::Knob(num) => U7::cull(0x20 + u8::from(*num)),
            Encoder::JogWheel => U7::cull(0x30),
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

/// Pressure-sensitive pad config
#[derive(Debug)]
pub struct Beatstep {
    endpoint: Interface,
    device_id: u8,
}



impl Beatstep {
    pub fn get_pad(&self, pad: PadNum) -> PadMode {
        // self.
        PadMode::PadOff(pad)
    }

    // PadOff(Pad),
    pub fn set_pad_off(&self, pad: Pad) {}

    // PadMMC(Pad, MMC),
    pub fn set_pad_mmc(&self, pad: Pad, mmc: MMC) {}

    // PadCC(Pad, Channel, Control, OnValue, OffValue, SwitchMode),
    pub fn set_pad_cc(&self, pad: Pad, channel: Channel, control: Control, on_value: OnValue, off_value: OffValue, switch_mode: SwitchMode) {}

    // PadCCSilent(Pad, Channel, Control, OnValue, OffValue, SwitchMode),
    pub fn set_pad_cc_cilent(&self, pad: Pad, channel: Channel, control: Control, on_value: OnValue, off_value: OffValue, switch_mode: SwitchMode) {}

    // PadNote(Pad, Channel, Note),
    pub fn set_pad_note(&self, pad: Pad, channel: Channel, note: Note) {}

    // PadProgramChange(Pad, Channel, Program, BankLSB, BankMSB),
    pub fn set_pad_program_change(&self, pad: Pad, channel: Channel, program: Program, bank_lsb: BankLSB, bank_msb: BankMSB) {}

    // Turn On / Off pad LED if it is set to Note mode. Works by sending NoteOn / NoteOff
    pub fn pad_led_enabled(channel: Channel, note: Note, on: bool) {}

    // GlobalMidiChannel(Channel),
    // CVGateChannel(Channel),
    // KnobAcceleration(Acceleration),
    // PadVelocityCurve(VelocityCurve),
    //
    // StepNote(Channel, StepNum, Note),
    // StepEnabled(Channel, StepNum, bool),

    // fn mode_code(&self) -> Result<U7, MidiError> {
    //     Ok(match self {
    //         Beatstep::PadOff(_) | Beatstep::KnobOff(_) => U7::cull(0),
    //         Beatstep::PadMMC(..) => U7::cull(7),
    //         Beatstep::PadCC(..) => U7::cull(8),
    //         Beatstep::PadCCSilent(..) | Beatstep::KnobCC(..) => U7::cull(1),
    //         Beatstep::PadNote(..) => U7::cull(9),
    //         Beatstep::PadProgramChange(..) => U7::cull(0xB),
    //
    //         Beatstep::KnobOff(_) => U7::cull(0),
    //         Beatstep::KnobNRPN(..) => U7::cull(4),
    //
    //         _ => Err(MidiError::NoModeForParameter)?
    //     })
    // }
}

#[derive(Debug)]
pub enum PadMode {
    PadOff(PadNum),
    PadCC(PadNum, Channel, Control, OnValue, OffValue, SwitchMode),
    PadCCSilent(PadNum, Channel, Control, OnValue, OffValue, SwitchMode),
    PadNote(PadNum, Channel, Note),
    PadProgramChange(PadNum, Channel, Program, BankLSB, BankMSB),
}

#[derive(Debug)]
pub enum EncoderMode {
    KnobOff(Encoder),
    KnobCC(Encoder, Channel, Control, Minimum, Maximum, Behavior),
    KnobNRPN(Encoder, Channel, Granularity, BankLSB, BankMSB, NRPNType),
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



