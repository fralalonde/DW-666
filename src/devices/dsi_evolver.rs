//! From https://www.untergeek.de/2014/11/taming-arturias-beatstep-sysex-codes-for-programming-via-ipad/
//! Thanks to Richard Wanderlöf and Untergeek
//! Switching the LEDs on and off:

use crate::midi::{U7, U4, Note, Program, Control, Channel, Cull, MidiError, ResponseMatcher, ResponseToken, Tag};
use ResponseToken::{Ref, Capture};
use Tag::*;

// const GET_CTL_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x01, 0x00, /* param, control */];
//
// const SET_CTL_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, /* param, control, value */];
//
// const GET_SEQ_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x01, 0x00, /* param, step */];
//
// const SET_SEQ_HEADER: &'static [u8] = &[00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, /* param, step, value */];

const SEQUENTIAL: u8 = 0x01;
const EVOLVER: u8 = 0x20;
const PROGRAM_PARAM: &'static [u8] = &[SEQUENTIAL, EVOLVER, 0x01, 0x01];

pub fn program_parameter_matcher() -> ResponseMatcher {
    ResponseMatcher::new(&[Ref(PROGRAM_PARAM), Capture(ParamId), Capture(LsbValueU4), Capture(MsbValueU4)])
}


// #[derive(Debug)]
// #[repr(u8)]
// pub enum MMC {
//     Stop = 1,
//     Play = 2,
//     DeferredPlay = 3,
//     FastForward = 4,
//     Rewind = 5,
//     RecordStrobe = 6,
//     RecordExit = 7,
//     RecordReady = 8,
//     Pause = 9,
//     Eject = 10,
//     Chase = 11,
//     InListReset = 12,
// }
//
// pub type PadNum = U4;
//
// #[derive(Debug)]
// pub enum Pad {
//     Pad(PadNum),
//     Start,
//     Stop,
//     CtrlSeq,
//     ExtSync,
//     Recall,
//     Store,
//     Shift,
//     Chan,
// }
//
// trait ControlCode {
//     fn control_code(&self) -> U7;
// }
//
// impl ControlCode for Pad {
//     fn control_code(&self) -> U7 {
//         match self {
//             Pad::Pad(num) => U7::cull(0x70 + u8::from(*num)),
//             Pad::Start => U7::cull(0x70),
//             Pad::Stop => U7::cull(0x58),
//             Pad::CtrlSeq => U7::cull(0x5A),
//             Pad::ExtSync => U7::cull(0x5B),
//             Pad::Recall => U7::cull(0x5C),
//             Pad::Store => U7::cull(0x5D),
//             Pad::Shift => U7::cull(0x5E),
//             Pad::Chan => U7::cull(0x5F),
//         }
//     }
// }
//
// impl ControlCode for Encoder {
//     fn control_code(&self) -> U7 {
//         match self {
//             Encoder::Knob(num) => U7::cull(0x20 + u8::from(*num)),
//             Encoder::JogWheel => U7::cull(0x30),
//         }
//     }
// }
//
// pub type OnValue = U7;
// pub type OffValue = U7;
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum SwitchMode {
//     Toggle = 0,
//     Gate = 1,
// }
//
// pub type BankLSB = U7;
// pub type BankMSB = U7;
// pub type StepNum = U4;
//
// /// Pressure-sensitive pad config
// #[derive(Debug)]
// pub enum Beatstep {
//     PadOff(Pad),
//     PadMMC(Pad, MMC),
//     PadCC(Pad, Channel, Control, OnValue, OffValue, SwitchMode),
//     PadCCSilent(Pad, Channel, Control, OnValue, OffValue, SwitchMode),
//     PadNote(Pad, Channel, Note),
//     PadProgramChange(Pad, Channel, Program, BankLSB, BankMSB),
//
//     KnobOff(Encoder),
//     KnobCC(Encoder, Channel, Control, Minimum, Maximum, Behavior),
//     KnobNRPN(Encoder, Channel, Granularity, BankLSB, BankMSB, NRPNType),
//
//     GlobalMidiChannel(Channel),
//     CVGateChannel(Channel),
//     KnobAcceleration(Acceleration),
//     PadVelocityCurve(VelocityCurve),
//
//     StepNote(Channel, StepNum, Note),
//     StepEnabled(Channel, StepNum, bool),
//
// }
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum PadSwitchParam {
//     Channel = 2,
//     Control = 3,
//     Off = 4,
//     On = 5,
//     SwitchMode = 6,
// }
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum PadNoteParam {
//     Channel = 2,
//     Note = 3,
//     SwitchMode = 6,
// }
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum PadProgramParam {
//     Channel = 2,
//     Program = 3,
//     BankLSB = 4,
//     BankMSB = 5,
// }
//
// impl Beatstep {
//     fn mode_code(&self) -> Result<U7, MidiError> {
//         Ok(match self {
//             Beatstep::PadOff(_) | Beatstep::KnobOff(_) => U7::cull(0),
//             Beatstep::PadMMC(..) => U7::cull(7),
//             Beatstep::PadCC(..) => U7::cull(8),
//             Beatstep::PadCCSilent(..) | Beatstep::KnobCC(..) => U7::cull(1),
//             Beatstep::PadNote(..) => U7::cull(9),
//             Beatstep::PadProgramChange(..) => U7::cull(0xB),
//
//             Beatstep::KnobOff(_) => U7::cull(0),
//             Beatstep::KnobNRPN(..) => U7::cull(4),
//
//             _ => Err(MidiError::NoModeForParameter)?
//         })
//     }
// }
//
// /// Rotary Encoder config
//
// pub type KnobNum = U4;
//
// #[derive(Debug)]
// pub enum Encoder {
//     Knob(KnobNum),
//     JogWheel,
// }
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum EncoderControlParam {
//     Channel = 2,
//     Control = 3,
//     Minimum = 4,
//     Maximum = 5,
//     Behavior = 6,
// }
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum EncoderNRPNParam {
//     Channel = 2,
//     Granularity = 3,
//     Minimum = 4,
//     Maximum = 5,
//     Behavior = 6,
// }
//
//
// pub type Minimum = U7;
// pub type Maximum = U7;
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum Behavior {
//     Absolute = 0,
//     RelativeCentered64 = 1,
//     RelativeCentered0 = 2,
//     RelativeCentered16 = 3,
// }
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum Granularity {
//     /// Controls MSB
//     Coarse = 0x06,
//     /// Controls LSB
//     Fine = 0x26,
// }
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum NRPNType {
//     /// Controls MSB
//     NRPN = 0,
//     /// Controls LSB
//     RPN = 1,
// }
//
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum Acceleration {
//     Slow = 0,
//     Medium = 1,
//     Fast = 2,
// }
//
// #[derive(Debug)]
// #[repr(u8)]
// pub enum VelocityCurve {
//     Linear = 0,
//     Logarithmic = 1,
//     Exponential = 2,
//     Full = 3,
// }
//
//
//
