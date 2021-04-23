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

pub fn parameter(param: u8, value: u8) -> Sysex {
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
#[derive(Debug, Copy, Clone)]
pub enum Param {
    Osc1Wave,
    Osc1Level,
    Osc1Octave,
    Osc2Wave,
    Osc2Level,
    Osc2Octave,
    Osc2Detune,
    Osc2Interval,
    NoiseLevel,
    Cutoff,
    Resonance,
    VcfEgInt,
    VcfEgAttack,
    VcfEgDecay,
    VcfEgBreakpoint,
    VcfEgSlope,
    VcfEgSustain,
    VcfEgRelease,
    VcaEgAttack,
    VcaEgDecay,
    VcaEgBreakpoint,
    VcaEgSlope,
    VcaEgSustain,
    VcaEgRelease,
    BendVcf,
    BendOsc,
    AssignMode,
    PortamentoTime,
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
    pub portamento_time: PortamentoTime,
    pub osc1_level: Osc1Level,
    pub osc2_level: Osc2Level,
    pub noise_level: NoiseLevel,

    pub cutoff: Cutoff,
    pub resonance: Resonance,

    pub vcf_eg_int: VcfEgInt,
    pub vcf_eg_attack: VcfEgAttack,
    pub vcf_eg_decay: VcfEgDecay,
    pub vcf_eg_breakpoint: VcfEgBreakpoint,
    pub vcf_eg_slope: VcfEgSlope,
    pub vcf_eg_sustain: VcfEgSustain,
    pub vcf_eg_release: VcfEgRelease,

    pub vca_eg_attack: VcaEgAttack,
    pub vca_eg_decay: VcaEgDecay,
    pub vca_eg_breakpoint: VcaEgBreakpoint,
    pub vca_eg_slope: VcaEgSlope,
    pub bend_vcf_vca_eg_sustain: BendVcfVcaEgSustain,
    pub osc1_oct_vca_eg_release: Osc1OctVcaEgRelease,

    pub osc2_oct_mg_freq: Osc2OctMgFreq,
    pub kbd_track_mg_delay: KbdTrackMgDelay,
    pub polarity_mg_osc: PolarityMgOsc,
    pub chorus_mg_vcf: ChrorusMgVcf,

    pub osc1_wave_osc2_wave: Osc1WaveOsc2Wave,
    pub osc2_interval_osc2_detune: Osc2IntervalOsc2Detune,
}

fn as_dump_ref_mut(buf: &mut [u8]) -> &mut Dump {
    let p: *mut Dump = buf.as_ptr() as *mut Dump;
    unsafe { &mut *p }
}

fn as_dump_ref(buf: &[u8]) -> &Dump {
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
        Param::Osc2Interval => dump.osc2_interval_osc2_detune.osc2_interval(),
        Param::NoiseLevel => dump.noise_level.noise_level(),
        Param::Cutoff => dump.cutoff.cutoff(),
        Param::Resonance => dump.resonance.resonance(),
        Param::VcfEgInt => dump.vcf_eg_int.vcf_eg_int(),
        Param::VcfEgAttack => dump.vcf_eg_attack.vcf_eg_attack(),
        Param::VcfEgDecay => dump.vcf_eg_decay.vcf_eg_decay(),
        Param::VcfEgBreakpoint => dump.vcf_eg_breakpoint.vcf_eg_breakpoint(),
        Param::VcfEgSlope => dump.vcf_eg_slope.vcf_eg_slope(),
        Param::VcfEgSustain => dump.vcf_eg_sustain.vcf_eg_sustain(),
        Param::VcfEgRelease => dump.vcf_eg_release.vcf_eg_release(),
        Param::VcaEgAttack => dump.vca_eg_attack.vca_eg_attack(),
        Param::VcaEgDecay => dump.vca_eg_decay.vca_eg_decay(),
        Param::VcaEgBreakpoint => dump.vca_eg_breakpoint.vca_eg_breakpoint(),
        Param::VcaEgSlope => dump.vca_eg_slope.vca_eg_slope(),
        Param::VcaEgSustain => dump.bend_vcf_vca_eg_sustain.vca_eg_sustain(),
        Param::VcaEgRelease => dump.osc1_oct_vca_eg_release.vca_eg_release(),
        Param::BendVcf => dump.bend_vcf_vca_eg_sustain.bend_vcf(),
        Param::BendOsc => dump.assign_mode_bend_osc.bend_osc(),
        Param::AssignMode => dump.assign_mode_bend_osc.assign_mode(),
        Param::PortamentoTime => dump.portamento_time.portamento_time(),
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
        Param::Osc2Interval => dump.osc2_interval_osc2_detune.set_osc2_interval(value),
        Param::NoiseLevel => dump.noise_level.set_noise_level(value),
        Param::Cutoff => dump.cutoff.set_cutoff(value),
        Param::Resonance => dump.resonance.set_resonance(value),
        Param::VcfEgInt => dump.vcf_eg_int.set_vcf_eg_int(value),
        Param::VcfEgAttack => dump.vcf_eg_attack.set_vcf_eg_attack(value),
        Param::VcfEgDecay => dump.vcf_eg_decay.set_vcf_eg_decay(value),
        Param::VcfEgBreakpoint => dump.vcf_eg_breakpoint.set_vcf_eg_breakpoint(value),
        Param::VcfEgSlope => dump.vcf_eg_slope.set_vcf_eg_slope(value),
        Param::VcfEgSustain => dump.vcf_eg_sustain.set_vcf_eg_sustain(value),
        Param::VcfEgRelease => dump.vcf_eg_release.set_vcf_eg_release(value),
        Param::VcaEgAttack => dump.vca_eg_attack.set_vca_eg_attack(value),
        Param::VcaEgDecay => dump.vca_eg_decay.set_vca_eg_decay(value),
        Param::VcaEgBreakpoint => dump.vca_eg_breakpoint.set_vca_eg_breakpoint(value),
        Param::VcaEgSlope => dump.vca_eg_slope.set_vca_eg_slope(value),
        Param::VcaEgSustain => dump.bend_vcf_vca_eg_sustain.set_vca_eg_sustain(value),
        Param::VcaEgRelease => dump.osc1_oct_vca_eg_release.set_vca_eg_release(value),
        Param::BendVcf => dump.bend_vcf_vca_eg_sustain.set_bend_vcf(value),
        Param::BendOsc => dump.assign_mode_bend_osc.set_bend_osc(value),
        Param::AssignMode => dump.assign_mode_bend_osc.set_assign_mode(value),
        Param::PortamentoTime => dump.portamento_time.set_portamento_time(value),
        Param::MgFreq => dump.osc2_oct_mg_freq.set_mg_freq(value),
        Param::MgDelay => dump.kbd_track_mg_delay.set_mg_delay(value),
        Param::MgOsc => dump.polarity_mg_osc.set_mg_osc(value),
        Param::MgVcf => dump.chorus_mg_vcf.set_mg_vcf(value),
        Param::KbdTrack => dump.kbd_track_mg_delay.set_kbd_track(value),
        Param::Polarity => dump.polarity_mg_osc.set_polarity(value),
        Param::Chorus => dump.chorus_mg_vcf.set_chrorus(value),
    }
}

pub fn param_to_sysex(param: Param, dump_buf: &[u8]) -> Sysex {
    let dump = as_dump_ref(dump_buf);
    match param {
        Param::AssignMode | Param::BendOsc => parameter(0, dump.assign_mode_bend_osc.0),
        Param::PortamentoTime => parameter(1, dump.portamento_time.0),
        Param::Osc1Level => parameter(2, dump.osc1_level.0),
        Param::Osc2Level => parameter(3, dump.osc2_level.0),
        Param::NoiseLevel => parameter(4, dump.noise_level.0),
        Param::Cutoff => parameter(5, dump.cutoff.0),
        Param::Resonance => parameter(6, dump.resonance.0),
        Param::VcfEgInt => parameter(7, dump.vcf_eg_int.0),
        Param::VcfEgAttack => parameter(8, dump.vcf_eg_attack.0),
        Param::VcfEgDecay => parameter(9, dump.vcf_eg_decay.0),
        Param::VcfEgBreakpoint => parameter(10, dump.vcf_eg_breakpoint.0),
        Param::VcfEgSlope => parameter(11, dump.vcf_eg_slope.0),
        Param::VcfEgSustain => parameter(12, dump.vcf_eg_sustain.0),
        Param::VcfEgRelease => parameter(13, dump.vcf_eg_release.0),
        Param::VcaEgAttack => parameter(14, dump.vca_eg_attack.0),
        Param::VcaEgDecay => parameter(15, dump.vca_eg_decay.0),
        Param::VcaEgBreakpoint => parameter(16, dump.vca_eg_breakpoint.0),
        Param::VcaEgSlope => parameter(17, dump.vca_eg_slope.0),
        Param::BendVcf | Param::VcaEgSustain => parameter(18, dump.bend_vcf_vca_eg_sustain.0),
        Param::Osc1Octave | Param::VcaEgRelease => parameter(19, dump.osc1_oct_vca_eg_release.0),
        Param::Osc2Octave | Param::MgFreq => parameter(20, dump.osc2_oct_mg_freq.0),
        Param::KbdTrack | Param::MgDelay => parameter(21, dump.kbd_track_mg_delay.0),
        Param::Polarity | Param::MgOsc => parameter(22, dump.polarity_mg_osc.0),
        Param::Chorus | Param::MgVcf => parameter(23, dump.chorus_mg_vcf.0),
        Param::Osc1Wave | Param::Osc2Wave  => parameter(24, dump.osc1_wave_osc2_wave.0),
        Param::Osc2Detune | Param::Osc2Interval => parameter(25, dump.osc2_interval_osc2_detune.0),
    }
}

bitfield! {
    pub struct AssignModeBendOsc(u8);
    impl Debug;
    pub assign_mode, set_assign_mode: 5, 4;
    pub bend_osc, set_bend_osc: 3, 0;
}

bitfield! {
    pub struct PortamentoTime(u8); impl Debug;
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
    pub struct NoiseLevel(u8); impl Debug;
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
    pub struct VcfEgInt(u8); impl Debug;
    pub vcf_eg_int, set_vcf_eg_int: 4,0;
}

bitfield! {
    pub struct VcfEgAttack(u8); impl Debug;
    pub vcf_eg_attack, set_vcf_eg_attack: 4,0;
}

bitfield! {
    pub struct VcfEgDecay(u8); impl Debug;
    pub vcf_eg_decay, set_vcf_eg_decay: 4,0;
}

bitfield! {
    pub struct VcfEgBreakpoint(u8); impl Debug;
    pub vcf_eg_breakpoint, set_vcf_eg_breakpoint: 4,0;
}

bitfield! {
    pub struct VcfEgSlope(u8); impl Debug;
    pub vcf_eg_slope, set_vcf_eg_slope: 4,0;
}

bitfield! {
    pub struct VcfEgSustain(u8); impl Debug;
    pub vcf_eg_sustain, set_vcf_eg_sustain: 4,0;
}

bitfield! {
    pub struct VcfEgRelease(u8); impl Debug;
    pub vcf_eg_release, set_vcf_eg_release: 4,0;
}

bitfield! {
    pub struct VcaEgAttack(u8); impl Debug;
    pub vca_eg_attack, set_vca_eg_attack: 4,0;
}

bitfield! {
    pub struct VcaEgDecay(u8); impl Debug;
    pub vca_eg_decay, set_vca_eg_decay: 4,0;
}

bitfield! {
    pub struct VcaEgBreakpoint(u8); impl Debug;
    pub vca_eg_breakpoint, set_vca_eg_breakpoint: 4,0;
}

bitfield! {
    pub struct VcaEgSlope(u8); impl Debug;
    pub vca_eg_slope, set_vca_eg_slope: 4,0;
}

bitfield! {
    pub struct BendVcfVcaEgSustain(u8); impl Debug;
    pub bend_vcf, set_bend_vcf: 5,5;
    pub vca_eg_sustain, set_vca_eg_sustain: 4,0;
}

bitfield! {
    pub struct Osc1OctVcaEgRelease(u8); impl Debug;
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
    pub struct Osc2IntervalOsc2Detune(u8); impl Debug;
    pub osc2_interval, set_osc2_interval: 5,3;
    pub osc2_detune, set_osc2_detune: 2,0;
}
