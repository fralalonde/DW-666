//! From https://www.untergeek.de/2014/11/taming-arturias-beatstep-sysex-codes-for-programming-via-ipad/
//! Thanks to Richard Wanderlöf and Untergeek
//! Switching the LEDs on and off:

use crate::midi::{Matcher, Token, Tag, Sysex};
use Token::{Seq, Cap, Val, Buf};
use Tag::*;
use alloc::vec::Vec;

const KORG: u8 = 0x42;
const DW_6000: u8 = 0x04;

const ID_FORMAT: u8 = 0x40;
const DATA_FORMAT: u8 = 0x30;

const WRITE_OK: u8 = 0x21;
const WRITE_ERR: u8 = 0x22;

const ID_HEADER: &'static [u8] = &[KORG, ID_FORMAT];
const DATA_HEADER: &'static [u8] = &[KORG, DATA_FORMAT, DW_6000];

pub fn id_request() -> Sysex {
    Sysex::new(vec![Seq(ID_HEADER)])
}

pub fn id_matcher() -> Matcher {
    Matcher::new(vec![Seq(ID_HEADER), Val(DW_6000)])
}

pub fn write(program: u8) -> Sysex {
    Sysex::new(vec![Seq(DATA_HEADER), Val(0x11), Val(program)])
}

pub fn load(dump: Vec<u8>) -> Sysex {
    Sysex::new(vec![Seq(DATA_HEADER), Buf(dump)])
}

pub fn parameter_set(param: u8, value: u8) -> Sysex {
    Sysex::new(vec![Seq(DATA_HEADER), Val(0x41), Val(param), Val(value)])
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

#[allow(unused)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Param {
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

#[repr(C, packed)]
#[derive(Debug)]
pub struct Dump {
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

pub fn as_dump_ref_mut(buf: &mut [u8]) -> &mut Dump {
    let p: *mut Dump = buf.as_ptr() as *mut Dump;
    unsafe { &mut *p }
}

pub fn as_dump_ref(buf: &[u8]) -> &Dump {
    let p: *const Dump = buf.as_ptr() as *const Dump;
    unsafe { &*p }
}

pub fn get_param_value(param: Param, dump_buf: &[u8]) -> u8 {
    let dump = as_dump_ref(dump_buf);
    match param {
        Param::Osc1Wave => dump.osc1_wave_osc2_wave.osc1_waveform(),
        Param::Osc1Level => dump.osc1_level.osc1_level(),
        Param::Osc1Octave => dump.osc1_oct_vca_eg_release.osc1_octave(),
        Param::Osc2Wave => dump.osc1_wave_osc2_wave.osc2_waveform(),
        Param::Osc2Level => dump.osc2_level.osc2_level(),
        Param::Osc2Octave => dump.osc2_oct_mg_freq.osc2_octave(),
        Param::Osc2Detune => dump.osc2_interval_osc2_detune.osc2_detune(),
        Param::Interval => dump.osc2_interval_osc2_detune.osc2_interval(),
        Param::Noise => dump.noise_level.noise_level(),
        Param::Cutoff => dump.cutoff.cutoff(),
        Param::Resonance => dump.resonance.resonance(),
        Param::VcfInt => dump.vcf_eg_int.vcf_eg_int(),
        Param::VcfAttack => dump.vcf_eg_attack.vcf_eg_attack(),
        Param::VcfDecay => dump.vcf_eg_decay.vcf_eg_decay(),
        Param::VcfBreak => dump.vcf_eg_breakpoint.vcf_eg_breakpoint(),
        Param::VcfSlope => dump.vcf_eg_slope.vcf_eg_slope(),
        Param::VcfSustain => dump.vcf_eg_sustain.vcf_eg_sustain(),
        Param::VcfRelease => dump.vcf_eg_release.vcf_eg_release(),
        Param::VcaAttack => dump.vca_eg_attack.vca_eg_attack(),
        Param::VcaDecay => dump.vca_eg_decay.vca_eg_decay(),
        Param::VcaBreak => dump.vca_eg_breakpoint.vca_eg_breakpoint(),
        Param::VcaSlope => dump.vca_eg_slope.vca_eg_slope(),
        Param::VcaSustain => dump.bend_vcf_vca_eg_sustain.vca_eg_sustain(),
        Param::VcaRelease => dump.osc1_oct_vca_eg_release.vca_eg_release(),
        Param::BendVcf => dump.bend_vcf_vca_eg_sustain.bend_vcf(),
        Param::BendOsc => dump.assign_mode_bend_osc.bend_osc(),
        Param::AssignMode => dump.assign_mode_bend_osc.assign_mode(),
        Param::Portamento => dump.portamento_time.portamento_time(),
        Param::MgFreq => dump.osc2_oct_mg_freq.mg_freq(),
        Param::MgDelay => dump.kbd_track_mg_delay.mg_delay(),
        Param::MgOsc => dump.polarity_mg_osc.mg_osc(),
        Param::MgVcf => dump.chorus_mg_vcf.mg_vcf(),
        Param::KbdTrack => dump.kbd_track_mg_delay.kbd_track(),
        Param::Polarity => dump.polarity_mg_osc.polarity(),
        Param::Chorus => dump.chorus_mg_vcf.chorus(),
    }
}

pub fn set_param_value(param: Param, value: u8, dump_buf: &mut [u8]) {
    let dump = as_dump_ref_mut(dump_buf);
    match param {
        Param::Osc1Wave => dump.osc1_wave_osc2_wave.set_osc1_waveform(value),
        Param::Osc1Level => dump.osc1_level.set_osc1_level(value),
        Param::Osc1Octave => dump.osc1_oct_vca_eg_release.set_osc1_octave(value),
        Param::Osc2Wave => dump.osc1_wave_osc2_wave.set_osc2_waveform(value),
        Param::Osc2Level => dump.osc2_level.set_osc2_level(value),
        Param::Osc2Octave => dump.osc2_oct_mg_freq.set_osc2_octave(value),
        Param::Osc2Detune => dump.osc2_interval_osc2_detune.set_osc2_detune(value),
        Param::Interval => dump.osc2_interval_osc2_detune.set_osc2_interval(value),
        Param::Noise => dump.noise_level.set_noise_level(value),
        Param::Cutoff => dump.cutoff.set_cutoff(value),
        Param::Resonance => dump.resonance.set_resonance(value),
        Param::VcfInt => dump.vcf_eg_int.set_vcf_eg_int(value),
        Param::VcfAttack => dump.vcf_eg_attack.set_vcf_eg_attack(value),
        Param::VcfDecay => dump.vcf_eg_decay.set_vcf_eg_decay(value),
        Param::VcfBreak => dump.vcf_eg_breakpoint.set_vcf_eg_breakpoint(value),
        Param::VcfSlope => dump.vcf_eg_slope.set_vcf_eg_slope(value),
        Param::VcfSustain => dump.vcf_eg_sustain.set_vcf_eg_sustain(value),
        Param::VcfRelease => dump.vcf_eg_release.set_vcf_eg_release(value),
        Param::VcaAttack => dump.vca_eg_attack.set_vca_eg_attack(value),
        Param::VcaDecay => dump.vca_eg_decay.set_vca_eg_decay(value),
        Param::VcaBreak => dump.vca_eg_breakpoint.set_vca_eg_breakpoint(value),
        Param::VcaSlope => dump.vca_eg_slope.set_vca_eg_slope(value),
        Param::VcaSustain => dump.bend_vcf_vca_eg_sustain.set_vca_eg_sustain(value),
        Param::VcaRelease => dump.osc1_oct_vca_eg_release.set_vca_eg_release(value),
        Param::BendVcf => dump.bend_vcf_vca_eg_sustain.set_bend_vcf(value),
        Param::BendOsc => dump.assign_mode_bend_osc.set_bend_osc(value),
        Param::AssignMode => dump.assign_mode_bend_osc.set_assign_mode(value),
        Param::Portamento => dump.portamento_time.set_portamento_time(value),
        Param::MgFreq => dump.osc2_oct_mg_freq.set_mg_freq(value),
        Param::MgDelay => dump.kbd_track_mg_delay.set_mg_delay(value),
        Param::MgOsc => dump.polarity_mg_osc.set_mg_osc(value),
        Param::MgVcf => dump.chorus_mg_vcf.set_mg_vcf(value),
        Param::KbdTrack => dump.kbd_track_mg_delay.set_kbd_track(value),
        Param::Polarity => dump.polarity_mg_osc.set_polarity(value),
        Param::Chorus => dump.chorus_mg_vcf.set_chrorus(value),
    }
}

impl Param {
    pub fn dump_index(&self) -> usize {
        match self {
            Param::AssignMode | Param::BendOsc => 0,
            Param::Portamento => 1,
            Param::Osc1Level => 2,
            Param::Osc2Level => 3,
            Param::Noise => 4,
            Param::Cutoff => 5,
            Param::Resonance => 6,
            Param::VcfInt => 7,
            Param::VcfAttack => 8,
            Param::VcfDecay => 9,
            Param::VcfBreak => 10,
            Param::VcfSlope => 11,
            Param::VcfSustain => 12,
            Param::VcfRelease => 13,
            Param::VcaAttack => 14,
            Param::VcaDecay => 15,
            Param::VcaBreak => 16,
            Param::VcaSlope => 17,
            Param::BendVcf | Param::VcaSustain => 18,
            Param::Osc1Octave | Param::VcaRelease => 19,
            Param::Osc2Octave | Param::MgFreq => 20,
            Param::KbdTrack | Param::MgDelay => 21,
            Param::Polarity | Param::MgOsc => 22,
            Param::Chorus | Param::MgVcf => 23,
            Param::Osc1Wave | Param::Osc2Wave  => 24,
            Param::Osc2Detune | Param::Interval => 25,
        }
    }

    pub fn dump_value(&self, dump_buf: &[u8]) -> u8 {
        let dump = as_dump_ref(dump_buf);
        match self {
            Param::AssignMode | Param::BendOsc => dump.assign_mode_bend_osc.0,
            Param::Portamento => dump.portamento_time.0,
            Param::Osc1Level => dump.osc1_level.0,
            Param::Osc2Level => dump.osc2_level.0,
            Param::Noise => dump.noise_level.0,
            Param::Cutoff => dump.cutoff.0,
            Param::Resonance => dump.resonance.0,
            Param::VcfInt => dump.vcf_eg_int.0,
            Param::VcfAttack => dump.vcf_eg_attack.0,
            Param::VcfDecay => dump.vcf_eg_decay.0,
            Param::VcfBreak => dump.vcf_eg_breakpoint.0,
            Param::VcfSlope => dump.vcf_eg_slope.0,
            Param::VcfSustain => dump.vcf_eg_sustain.0,
            Param::VcfRelease => dump.vcf_eg_release.0,
            Param::VcaAttack => dump.vca_eg_attack.0,
            Param::VcaDecay => dump.vca_eg_decay.0,
            Param::VcaBreak => dump.vca_eg_breakpoint.0,
            Param::VcaSlope => dump.vca_eg_slope.0,
            Param::BendVcf | Param::VcaSustain => dump.bend_vcf_vca_eg_sustain.0,
            Param::Osc1Octave | Param::VcaRelease => dump.osc1_oct_vca_eg_release.0,
            Param::Osc2Octave | Param::MgFreq => dump.osc2_oct_mg_freq.0,
            Param::KbdTrack | Param::MgDelay => dump.kbd_track_mg_delay.0,
            Param::Polarity | Param::MgOsc => dump.polarity_mg_osc.0,
            Param::Chorus | Param::MgVcf => dump.chorus_mg_vcf.0,
            Param::Osc1Wave | Param::Osc2Wave  => dump.osc1_wave_osc2_wave.0,
            Param::Osc2Detune | Param::Interval => dump.osc2_interval_osc2_detune.0,
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
