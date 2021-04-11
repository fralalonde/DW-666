//! From https://www.untergeek.de/2014/11/taming-arturias-beatstep-sysex-codes-for-programming-via-ipad/
//! Thanks to Richard Wanderlöf and Untergeek
//! Switching the LEDs on and off:

use crate::midi::{U7, U4, U6, Note, Program, Control, Channel, Cull, MidiError, ResponseMatcher, ResponseToken, Tag, RequestSequence};
use ResponseToken::{Ref, Capture, Val};
use Tag::*;
use core::iter::Repeat;

pub type DUMP = [u8; 26];

const KORG: u8 = 0x42;
const FORMAT: u8 = 0x30;
const DW_6000: u8 = 0x04;

const WRITE_OK: u8 = 0x21;
const WRITE_ERR: u8 = 0x22;

const DEVICE_ID: &'static [u8] = &[KORG, FORMAT, DW_6000];

#[derive(Default)]
pub struct Dump ([u8;26]);

static DUMP_BUFFERS: [Dump; 2] = [Dump::default(); 2];

pub fn device_id_request() -> RequestSequence {
    RequestSequence::new(&[Ref(&[KORG, FORMAT])])
}

pub fn device_id_matcher() -> ResponseMatcher {
    ResponseMatcher::new(&[Ref(DEVICE_ID)])
}

pub fn write_request(program: U6) -> RequestSequence {
    RequestSequence::new(&[Ref(DEVICE_ID), Val(0x11), Val(program.into())])
}

pub fn write_response() -> ResponseMatcher {
    ResponseMatcher::new(&[Ref(DEVICE_ID), Capture(Tag::ValueU7)])
}

pub fn dump_write_request(dump: &'static Dump) -> RequestSequence {
    RequestSequence::new(&[Ref(DEVICE_ID), Ref(dump)])
}

pub fn param_write_request(param: u8, value: U7) -> RequestSequence {
    RequestSequence::new(&[Ref(DEVICE_ID), Val(0x41), Val(param.into()), Val(value.into())])
}

pub fn dump_read_request() -> RequestSequence {
    RequestSequence::new(&[Ref(DEVICE_ID), Val(0x10)])
}

pub fn dump_read_response(dump: &'static Dump) -> ResponseMatcher {
    ResponseMatcher::new(&[Ref(DEVICE_ID), Val(0x40), ResponseToken::CaptureArray(Tag::Dump, 26)])
}

pub enum Param {
    AssignModeBendOsc = 0,
    PortamentoTime = 1,

    Osc1Level = 2,
    Osc2Level = 3,
    NoiseLevel = 4,

    Cutoff = 5,
    Resonance = 6,

    VcfEgInt = 7,
    VcfEgAttack = 8,
    VcfEgDecay = 9,
    VcfEgBreakpoint = 10,
    VcfEgSlope = 11,
    VcfEgSustain = 12,
    VcfEgRelease = 13,

    VcaEgAttack = 14,
    VcaEgDecay = 15,
    VcaEgBreakpoint = 16,
    VcaEgSlope = 17,
    BendVcfVcaEgSustain = 18,
    Osc1OctaveVcaEgRelease = 19,

    Osc2OctaveMgFreq = 20,
    KbdTrackMgDelay = 21,
    PolarityMgOsc = 22,
    ChorusMgVcf = 23,

    Osc1Osc2Waveform = 24,
    Osc2IntervalDetune = 25,
}

bitfield! {
    pub struct AssignModeBendOsc(u8);
    impl Debug;
    assign_mode, set_assign_mode: 5, 4;
    bend_osc, set_bend_osc: 3, 0;
}

bitfield! {
    pub struct ChorusMgVcf(u8);
    impl Debug;
    chrous, set_chorus: 5;
    mg_vcf, set_mg_vcf: 4, 0;
}

//
// pub fn is_chorus_enabled() -> (RequestSequence, ResponseMatcher) {
//   dump_loader()
// }