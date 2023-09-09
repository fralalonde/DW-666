//! From https://www.untergeek.de/2014/11/taming-arturias-beatstep-sysex-codes-for-programming-via-ipad/
//! Thanks to Richard Wanderlöf and Untergeek
//! Switching the LEDs on and off:
#![allow(dead_code)]

use crate::sysex::{SysexMatcher, Token, Tag, SysexSeq};
use Token::{Seq, Cap, Val, Buf};
use Tag::*;
use alloc::vec::Vec;

const KORG: u8 = 0x42;
const DW_6000: u8 = 0x04;

const ID_FORMAT: u8 = 0x40;
const DATA_FORMAT: u8 = 0x30;

const WRITE_OK: u8 = 0x21;
const WRITE_ERR: u8 = 0x22;

const ID_HEADER: &[u8] = &[KORG, ID_FORMAT];
const DATA_HEADER: &[u8] = &[KORG, DATA_FORMAT, DW_6000];

pub fn id_request_sysex() -> SysexSeq {
    SysexSeq::new(vec![Seq(ID_HEADER)])
}

pub fn id_matcher() -> SysexMatcher {
    SysexMatcher::new(vec![Seq(ID_HEADER), Val(DW_6000)])
}

pub fn write_program_sysex(program: u8) -> SysexSeq {
    SysexSeq::new(vec![Seq(DATA_HEADER), Val(0x11), Val(program)])
}

pub fn load_program_sysex(dump: Vec<u8>) -> SysexSeq {
    SysexSeq::new(vec![Seq(DATA_HEADER), Buf(dump)])
}

pub fn set_parameter_sysex(param: u8, value: u8) -> SysexSeq {
    SysexSeq::new(vec![Seq(DATA_HEADER), Val(0x41), Val(param), Val(value)])
}

pub fn write_matcher() -> SysexMatcher {
    SysexMatcher::new(vec![Seq(DATA_HEADER), Cap(ValueU7)])
}

pub fn dump_request_sysex() -> SysexSeq {
    SysexSeq::new(vec![Seq(DATA_HEADER), Val(0x10)])
}

pub fn dump_matcher() -> SysexMatcher {
    SysexMatcher::new(vec![Seq(DATA_HEADER), Val(0x40), Cap(Dump(26))])
}

#[allow(unused)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Dw6Param {
    Osc1Wave,
    Osc1Level,
    Osc1Octave,
    Osc2Wave,
    Osc2Level,
    Osc2Octave,
    Osc2Detune,
    Interval,
    Noise,
    Cutoff,
    Resonance,
    VcfInt,
    VcfAttack,
    VcfDecay,
    VcfBreak,
    VcfSlope,
    VcfSustain,
    VcfRelease,
    VcaAttack,
    VcaDecay,
    VcaBreak,
    VcaSlope,
    VcaSustain,
    VcaRelease,
    BendVcf,
    BendOsc,
    AssignMode,
    Portamento,
    MgFreq,
    MgDelay,
    MgOsc,
    MgVcf,
    KbdTrack,
    Polarity,
    Chorus,
}

#[repr(C)]
#[derive(Debug)]
pub struct Dw6Dump {
    pub assign_mode_bend_osc: AssignModeBendOsc,
    pub portamento_time: Portamento,
    pub osc1_level: Osc1Level,
    pub osc2_level: Osc2Level,
    pub noise_level: Noise,

    pub cutoff: Cutoff,
    pub resonance: Resonance,

    pub vcf_eg_int: VcfInt,
    pub vcf_eg_attack: VcfAttack,
    pub vcf_eg_decay: VcfDecay,
    pub vcf_eg_breakpoint: VcfBreak,
    pub vcf_eg_slope: VcfSlope,
    pub vcf_eg_sustain: VcfSustain,
    pub vcf_eg_release: VcfRelease,

    pub vca_eg_attack: VcaAttack,
    pub vca_eg_decay: VcaDecay,
    pub vca_eg_breakpoint: VcaBreak,
    pub vca_eg_slope: VcaSlope,
    pub bend_vcf_vca_eg_sustain: BendVcfVcaSustain,
    pub osc1_oct_vca_eg_release: Osc1OctVcaRelease,

    pub osc2_oct_mg_freq: Osc2OctMgFreq,
    pub kbd_track_mg_delay: KbdTrackMgDelay,
    pub polarity_mg_osc: PolarityMgOsc,
    pub chorus_mg_vcf: ChrorusMgVcf,

    pub osc1_wave_osc2_wave: Osc1WaveOsc2Wave,
    pub osc2_interval_osc2_detune: IntervalOsc2Detune,
}

pub fn as_dump_ref_mut(buf: &[u8]) -> &mut Dw6Dump {
    let p: *mut Dw6Dump = buf.as_ptr() as *mut Dw6Dump;
    unsafe { &mut *p }
}

pub fn as_dump_ref(buf: &[u8]) -> &Dw6Dump {
    let p: *const Dw6Dump = buf.as_ptr() as *const Dw6Dump;
    unsafe { &*p }
}

pub fn get_param_value(param: Dw6Param, dump_buf: &[u8]) -> u8 {
    let dump = as_dump_ref(dump_buf);
    match param {
        Dw6Param::Osc1Wave => dump.osc1_wave_osc2_wave.osc1_waveform(),
        Dw6Param::Osc1Level => dump.osc1_level.osc1_level(),
        Dw6Param::Osc1Octave => dump.osc1_oct_vca_eg_release.osc1_octave(),
        Dw6Param::Osc2Wave => dump.osc1_wave_osc2_wave.osc2_waveform(),
        Dw6Param::Osc2Level => dump.osc2_level.osc2_level(),
        Dw6Param::Osc2Octave => dump.osc2_oct_mg_freq.osc2_octave(),
        Dw6Param::Osc2Detune => dump.osc2_interval_osc2_detune.osc2_detune(),
        Dw6Param::Interval => dump.osc2_interval_osc2_detune.osc2_interval(),
        Dw6Param::Noise => dump.noise_level.noise_level(),
        Dw6Param::Cutoff => dump.cutoff.cutoff(),
        Dw6Param::Resonance => dump.resonance.resonance(),
        Dw6Param::VcfInt => dump.vcf_eg_int.vcf_eg_int(),
        Dw6Param::VcfAttack => dump.vcf_eg_attack.vcf_eg_attack(),
        Dw6Param::VcfDecay => dump.vcf_eg_decay.vcf_eg_decay(),
        Dw6Param::VcfBreak => dump.vcf_eg_breakpoint.vcf_eg_breakpoint(),
        Dw6Param::VcfSlope => dump.vcf_eg_slope.vcf_eg_slope(),
        Dw6Param::VcfSustain => dump.vcf_eg_sustain.vcf_eg_sustain(),
        Dw6Param::VcfRelease => dump.vcf_eg_release.vcf_eg_release(),
        Dw6Param::VcaAttack => dump.vca_eg_attack.vca_eg_attack(),
        Dw6Param::VcaDecay => dump.vca_eg_decay.vca_eg_decay(),
        Dw6Param::VcaBreak => dump.vca_eg_breakpoint.vca_eg_breakpoint(),
        Dw6Param::VcaSlope => dump.vca_eg_slope.vca_eg_slope(),
        Dw6Param::VcaSustain => dump.bend_vcf_vca_eg_sustain.vca_eg_sustain(),
        Dw6Param::VcaRelease => dump.osc1_oct_vca_eg_release.vca_eg_release(),
        Dw6Param::BendVcf => dump.bend_vcf_vca_eg_sustain.bend_vcf(),
        Dw6Param::BendOsc => dump.assign_mode_bend_osc.bend_osc(),
        Dw6Param::AssignMode => dump.assign_mode_bend_osc.assign_mode(),
        Dw6Param::Portamento => dump.portamento_time.portamento_time(),
        Dw6Param::MgFreq => dump.osc2_oct_mg_freq.mg_freq(),
        Dw6Param::MgDelay => dump.kbd_track_mg_delay.mg_delay(),
        Dw6Param::MgOsc => dump.polarity_mg_osc.mg_osc(),
        Dw6Param::MgVcf => dump.chorus_mg_vcf.mg_vcf(),
        Dw6Param::KbdTrack => dump.kbd_track_mg_delay.kbd_track(),
        Dw6Param::Polarity => dump.polarity_mg_osc.polarity(),
        Dw6Param::Chorus => dump.chorus_mg_vcf.chorus(),
    }
}

pub fn set_param_value(param: Dw6Param, value: u8, dump_buf: &[u8]) {
    let dump = as_dump_ref_mut(dump_buf);
    match param {
        Dw6Param::Osc1Wave => dump.osc1_wave_osc2_wave.set_osc1_waveform(value),
        Dw6Param::Osc1Level => dump.osc1_level.set_osc1_level(value),
        Dw6Param::Osc1Octave => dump.osc1_oct_vca_eg_release.set_osc1_octave(value),
        Dw6Param::Osc2Wave => dump.osc1_wave_osc2_wave.set_osc2_waveform(value),
        Dw6Param::Osc2Level => dump.osc2_level.set_osc2_level(value),
        Dw6Param::Osc2Octave => dump.osc2_oct_mg_freq.set_osc2_octave(value),
        Dw6Param::Osc2Detune => dump.osc2_interval_osc2_detune.set_osc2_detune(value),
        Dw6Param::Interval => dump.osc2_interval_osc2_detune.set_osc2_interval(value),
        Dw6Param::Noise => dump.noise_level.set_noise_level(value),
        Dw6Param::Cutoff => dump.cutoff.set_cutoff(value),
        Dw6Param::Resonance => dump.resonance.set_resonance(value),
        Dw6Param::VcfInt => dump.vcf_eg_int.set_vcf_eg_int(value),
        Dw6Param::VcfAttack => dump.vcf_eg_attack.set_vcf_eg_attack(value),
        Dw6Param::VcfDecay => dump.vcf_eg_decay.set_vcf_eg_decay(value),
        Dw6Param::VcfBreak => dump.vcf_eg_breakpoint.set_vcf_eg_breakpoint(value),
        Dw6Param::VcfSlope => dump.vcf_eg_slope.set_vcf_eg_slope(value),
        Dw6Param::VcfSustain => dump.vcf_eg_sustain.set_vcf_eg_sustain(value),
        Dw6Param::VcfRelease => dump.vcf_eg_release.set_vcf_eg_release(value),
        Dw6Param::VcaAttack => dump.vca_eg_attack.set_vca_eg_attack(value),
        Dw6Param::VcaDecay => dump.vca_eg_decay.set_vca_eg_decay(value),
        Dw6Param::VcaBreak => dump.vca_eg_breakpoint.set_vca_eg_breakpoint(value),
        Dw6Param::VcaSlope => dump.vca_eg_slope.set_vca_eg_slope(value),
        Dw6Param::VcaSustain => dump.bend_vcf_vca_eg_sustain.set_vca_eg_sustain(value),
        Dw6Param::VcaRelease => dump.osc1_oct_vca_eg_release.set_vca_eg_release(value),
        Dw6Param::BendVcf => dump.bend_vcf_vca_eg_sustain.set_bend_vcf(value),
        Dw6Param::BendOsc => dump.assign_mode_bend_osc.set_bend_osc(value),
        Dw6Param::AssignMode => dump.assign_mode_bend_osc.set_assign_mode(value),
        Dw6Param::Portamento => dump.portamento_time.set_portamento_time(value),
        Dw6Param::MgFreq => dump.osc2_oct_mg_freq.set_mg_freq(value),
        Dw6Param::MgDelay => dump.kbd_track_mg_delay.set_mg_delay(value),
        Dw6Param::MgOsc => dump.polarity_mg_osc.set_mg_osc(value),
        Dw6Param::MgVcf => dump.chorus_mg_vcf.set_mg_vcf(value),
        Dw6Param::KbdTrack => dump.kbd_track_mg_delay.set_kbd_track(value),
        Dw6Param::Polarity => dump.polarity_mg_osc.set_polarity(value),
        Dw6Param::Chorus => dump.chorus_mg_vcf.set_chrorus(value),
    }
}

impl Dw6Param {
    pub fn max_value(&self) -> u8 {
        match self {
            Dw6Param::Osc2Detune | Dw6Param::Interval |
            Dw6Param::Osc1Wave | Dw6Param::Osc2Wave => 7,

            Dw6Param::AssignMode | Dw6Param::KbdTrack |
            Dw6Param::Osc1Octave | Dw6Param::Osc2Octave => 3,

            Dw6Param::Cutoff => 63,

            Dw6Param::Resonance |
            Dw6Param::Portamento |
            Dw6Param::Osc2Level | Dw6Param::Osc1Level | Dw6Param::Noise |
            Dw6Param::MgFreq | Dw6Param::MgDelay | Dw6Param::MgOsc | Dw6Param::MgVcf |
            Dw6Param::VcfInt | Dw6Param::VcfAttack | Dw6Param::VcfDecay | Dw6Param::VcfBreak | Dw6Param::VcfSlope | Dw6Param::VcfSustain | Dw6Param::VcfRelease |
            Dw6Param::VcaAttack | Dw6Param::VcaDecay | Dw6Param::VcaBreak | Dw6Param::VcaSlope | Dw6Param::VcaSustain | Dw6Param::VcaRelease => 31,

            Dw6Param::Polarity | Dw6Param::Chorus | Dw6Param::BendVcf => 1,

            Dw6Param::BendOsc => 15,
        }
    }


    pub fn dump_index(&self) -> usize {
        match self {
            Dw6Param::AssignMode | Dw6Param::BendOsc => 0,
            Dw6Param::Portamento => 1,
            Dw6Param::Osc1Level => 2,
            Dw6Param::Osc2Level => 3,
            Dw6Param::Noise => 4,
            Dw6Param::Cutoff => 5,
            Dw6Param::Resonance => 6,
            Dw6Param::VcfInt => 7,
            Dw6Param::VcfAttack => 8,
            Dw6Param::VcfDecay => 9,
            Dw6Param::VcfBreak => 10,
            Dw6Param::VcfSlope => 11,
            Dw6Param::VcfSustain => 12,
            Dw6Param::VcfRelease => 13,
            Dw6Param::VcaAttack => 14,
            Dw6Param::VcaDecay => 15,
            Dw6Param::VcaBreak => 16,
            Dw6Param::VcaSlope => 17,
            Dw6Param::BendVcf | Dw6Param::VcaSustain => 18,
            Dw6Param::Osc1Octave | Dw6Param::VcaRelease => 19,
            Dw6Param::Osc2Octave | Dw6Param::MgFreq => 20,
            Dw6Param::KbdTrack | Dw6Param::MgDelay => 21,
            Dw6Param::Polarity | Dw6Param::MgOsc => 22,
            Dw6Param::Chorus | Dw6Param::MgVcf => 23,
            Dw6Param::Osc1Wave | Dw6Param::Osc2Wave => 24,
            Dw6Param::Osc2Detune | Dw6Param::Interval => 25,
        }
    }

    pub fn dump_value(&self, dump_buf: &[u8]) -> u8 {
        let dump = as_dump_ref(dump_buf);
        match self {
            Dw6Param::AssignMode | Dw6Param::BendOsc => dump.assign_mode_bend_osc.0,
            Dw6Param::Portamento => dump.portamento_time.0,
            Dw6Param::Osc1Level => dump.osc1_level.0,
            Dw6Param::Osc2Level => dump.osc2_level.0,
            Dw6Param::Noise => dump.noise_level.0,
            Dw6Param::Cutoff => dump.cutoff.0,
            Dw6Param::Resonance => dump.resonance.0,
            Dw6Param::VcfInt => dump.vcf_eg_int.0,
            Dw6Param::VcfAttack => dump.vcf_eg_attack.0,
            Dw6Param::VcfDecay => dump.vcf_eg_decay.0,
            Dw6Param::VcfBreak => dump.vcf_eg_breakpoint.0,
            Dw6Param::VcfSlope => dump.vcf_eg_slope.0,
            Dw6Param::VcfSustain => dump.vcf_eg_sustain.0,
            Dw6Param::VcfRelease => dump.vcf_eg_release.0,
            Dw6Param::VcaAttack => dump.vca_eg_attack.0,
            Dw6Param::VcaDecay => dump.vca_eg_decay.0,
            Dw6Param::VcaBreak => dump.vca_eg_breakpoint.0,
            Dw6Param::VcaSlope => dump.vca_eg_slope.0,
            Dw6Param::BendVcf | Dw6Param::VcaSustain => dump.bend_vcf_vca_eg_sustain.0,
            Dw6Param::Osc1Octave | Dw6Param::VcaRelease => dump.osc1_oct_vca_eg_release.0,
            Dw6Param::Osc2Octave | Dw6Param::MgFreq => dump.osc2_oct_mg_freq.0,
            Dw6Param::KbdTrack | Dw6Param::MgDelay => dump.kbd_track_mg_delay.0,
            Dw6Param::Polarity | Dw6Param::MgOsc => dump.polarity_mg_osc.0,
            Dw6Param::Chorus | Dw6Param::MgVcf => dump.chorus_mg_vcf.0,
            Dw6Param::Osc1Wave | Dw6Param::Osc2Wave => dump.osc1_wave_osc2_wave.0,
            Dw6Param::Osc2Detune | Dw6Param::Interval => dump.osc2_interval_osc2_detune.0,
        }
    }
}

bitfield! {
    pub struct AssignModeBendOsc(u8);
    impl Debug;
    pub assign_mode, set_assign_mode: 5, 4;
    pub bend_osc, set_bend_osc: 3, 0;
}

bitfield! {
    pub struct Portamento(u8); impl Debug;
    pub portamento_time, set_portamento_time: 4, 0;
}

bitfield! {
    pub struct Osc1Level(u8); impl Debug;
    pub osc1_level, set_osc1_level: 4, 0;
}

bitfield! {
    pub struct Osc2Level(u8); impl Debug;
    pub osc2_level, set_osc2_level: 4, 0;
}

bitfield! {
    pub struct Noise(u8); impl Debug;
    pub noise_level, set_noise_level: 4, 0;
}

bitfield! {
    pub struct Cutoff(u8); impl Debug;
    pub cutoff, set_cutoff: 5, 0;
}

bitfield! {
    pub struct Resonance(u8); impl Debug;
    pub resonance, set_resonance: 4, 0;
}

bitfield! {
    pub struct VcfInt(u8); impl Debug;
    pub vcf_eg_int, set_vcf_eg_int: 4,0;
}

bitfield! {
    pub struct VcfAttack(u8); impl Debug;
    pub vcf_eg_attack, set_vcf_eg_attack: 4,0;
}

bitfield! {
    pub struct VcfDecay(u8); impl Debug;
    pub vcf_eg_decay, set_vcf_eg_decay: 4,0;
}

bitfield! {
    pub struct VcfBreak(u8); impl Debug;
    pub vcf_eg_breakpoint, set_vcf_eg_breakpoint: 4,0;
}

bitfield! {
    pub struct VcfSlope(u8); impl Debug;
    pub vcf_eg_slope, set_vcf_eg_slope: 4,0;
}

bitfield! {
    pub struct VcfSustain(u8); impl Debug;
    pub vcf_eg_sustain, set_vcf_eg_sustain: 4,0;
}

bitfield! {
    pub struct VcfRelease(u8); impl Debug;
    pub vcf_eg_release, set_vcf_eg_release: 4,0;
}

bitfield! {
    pub struct VcaAttack(u8); impl Debug;
    pub vca_eg_attack, set_vca_eg_attack: 4,0;
}

bitfield! {
    pub struct VcaDecay(u8); impl Debug;
    pub vca_eg_decay, set_vca_eg_decay: 4,0;
}

bitfield! {
    pub struct VcaBreak(u8); impl Debug;
    pub vca_eg_breakpoint, set_vca_eg_breakpoint: 4,0;
}

bitfield! {
    pub struct VcaSlope(u8); impl Debug;
    pub vca_eg_slope, set_vca_eg_slope: 4,0;
}

bitfield! {
    pub struct BendVcfVcaSustain(u8); impl Debug;
    pub bend_vcf, set_bend_vcf: 5,5;
    pub vca_eg_sustain, set_vca_eg_sustain: 4,0;
}

bitfield! {
    pub struct Osc1OctVcaRelease(u8); impl Debug;
    pub osc1_octave, set_osc1_octave: 6,5;
    pub vca_eg_release, set_vca_eg_release: 4,0;
}

bitfield! {
    pub struct Osc2OctMgFreq(u8); impl Debug;
    pub osc2_octave, set_osc2_octave: 6,5;
    pub mg_freq, set_mg_freq: 4,0;
}

bitfield! {
    pub struct KbdTrackMgDelay(u8); impl Debug;
    pub kbd_track, set_kbd_track: 6,5;
    pub mg_delay, set_mg_delay: 4,0;
}

bitfield! {
    pub struct PolarityMgOsc(u8); impl Debug;
    pub polarity, set_polarity: 5,5;
    pub mg_osc, set_mg_osc: 4,0;
}

bitfield! {
    pub struct ChrorusMgVcf(u8); impl Debug;
    pub chorus, set_chrorus: 5,5;
    pub mg_vcf, set_mg_vcf: 4,0;
}

bitfield! {
    pub struct Osc1WaveOsc2Wave(u8); impl Debug;
    pub osc1_waveform, set_osc1_waveform: 5,3;
    pub osc2_waveform, set_osc2_waveform: 2,0;
}

bitfield! {
    pub struct IntervalOsc2Detune(u8); impl Debug;
    pub osc2_interval, set_osc2_interval: 5,3;
    pub osc2_detune, set_osc2_detune: 2,0;
}
