//! Sends MIDI to Korg DW-6000 acccording to messages
//!
use midi::{Message, Note, program_change, MidiError, U7, Endpoint};
use crate::route::{Router, Route,  Service, RouteContext};

use crate::{devices, time, midi, sysex};
use alloc::vec::Vec;
use alloc::sync::Arc;
use core::convert::TryFrom;
use crate::time::{BigInstant, TimeUnits, BigDuration, Tasks};
use time::long_now;
use devices::korg::dw6000::*;
use spin::MutexGuard;
use num_enum::TryFromPrimitive;
use num::{Integer};
use crate::apps::lfo::{Lfo, Waveform};

use crate::devices::korg::dw6000;
use crate::Binding::Dst;
use alloc::collections::BTreeMap;
use hashbrown::HashMap;
use crate::filter::capture_sysex;
use crate::sysex::Tag;

const SHORT_PRESS: u32 = 250;

pub struct Dw6000Control {
    state: Arc<spin::Mutex<InnerState>>,
}

impl Dw6000Control {
    pub fn new(dw6000: impl Into<Endpoint>, beatstep: impl Into<Endpoint>) -> Self {
        Dw6000Control {
            state: Arc::new(spin::Mutex::new(InnerState {
                dw6000: dw6000.into(),
                beatstep: beatstep.into(),
                current_dump: None,
                mod_dump: HashMap::new(),
                base_page: KnobPage::Osc,
                temp_page: None,
                bank: None,
                lfo2: Lfo::default(),
                lfo2_param: None,
            })),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
enum KnobPage {
    Osc = 0,
    Env = 1,
    Mod = 2,
    Arp = 3,
}


#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
enum TogglePage {
    Arp = 4,
    Latch = 5,
    Polarity = 6,
    Chorus = 7,
}

#[derive(Debug, Copy, Clone, TryFromPrimitive)]
#[repr(u8)]
enum Lfo2Dest {
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
    MgFreq,
    MgDelay,
    MgOsc,
    MgVcf,
}

impl From<Lfo2Dest> for Param {
    fn from(dest: Lfo2Dest) -> Self {
        match dest {
            Lfo2Dest::Osc1Wave => Param::Osc1Wave,
            Lfo2Dest::Osc1Level => Param::Osc1Level,
            Lfo2Dest::Osc1Octave => Param::Osc1Octave,
            Lfo2Dest::Osc2Wave => Param::Osc2Wave,
            Lfo2Dest::Osc2Level => Param::Osc2Level,
            Lfo2Dest::Osc2Octave => Param::Osc2Octave,
            Lfo2Dest::Osc2Detune => Param::Osc2Detune,
            Lfo2Dest::Interval => Param::Interval,
            Lfo2Dest::Noise => Param::Noise,
            Lfo2Dest::Cutoff => Param::Cutoff,
            Lfo2Dest::Resonance => Param::Resonance,
            Lfo2Dest::VcfInt => Param::VcfInt,
            Lfo2Dest::VcfAttack => Param::VcfAttack,
            Lfo2Dest::VcfDecay => Param::VcfDecay,
            Lfo2Dest::VcfBreak => Param::VcfBreak,
            Lfo2Dest::VcfSlope => Param::VcfSlope,
            Lfo2Dest::VcfSustain => Param::VcfSustain,
            Lfo2Dest::VcfRelease => Param::VcfRelease,
            Lfo2Dest::VcaAttack => Param::VcaAttack,
            Lfo2Dest::VcaDecay => Param::VcaDecay,
            Lfo2Dest::VcaBreak => Param::VcaBreak,
            Lfo2Dest::VcaSlope => Param::VcaSlope,
            Lfo2Dest::VcaSustain => Param::VcaSustain,
            Lfo2Dest::VcaRelease => Param::VcaRelease,
            Lfo2Dest::MgFreq => Param::MgFreq,
            Lfo2Dest::MgDelay => Param::MgDelay,
            Lfo2Dest::MgOsc => Param::MgOsc,
            Lfo2Dest::MgVcf => Param::MgVcf,
        }
    }
}

#[derive(Debug)]
enum ArpMode {
    Up,
    Down,
    UpDown,
}

#[derive(Debug)]
struct InnerState {
    dw6000: Endpoint,
    beatstep: Endpoint,

    current_dump: Option<Vec<u8>>,
    // saved values from dump before being modulated
    mod_dump: HashMap<Param, u8>,
    base_page: KnobPage,
    // if temp_page is released quickly, is becomes base_page
    temp_page: Option<(KnobPage, BigInstant)>,
    bank: Option<u8>,
    lfo2: Lfo,
    lfo2_param: Option<Lfo2Dest>,
    // arp_enabled: bool,
    // arp_mode: ArpMode,
    // arp_oct: u8, // 1..4
}

impl InnerState {
    fn active_page(&self) -> KnobPage {
        self.temp_page.map(|p| p.0).unwrap_or(self.base_page)
    }
}

fn note_page(note: Note) -> Option<KnobPage> {
    KnobPage::try_from(note as u8).ok()
}

fn toggle_page(note: Note) -> Option<TogglePage> {
    TogglePage::try_from(note as u8).ok()
}

fn note_bank(note: Note) -> Option<u8> {
    let note_u8 = note as u8;
    match note_u8.div_rem(&8) {
        (1, n) => Some(n),
        _ => None,
    }
}

fn note_prog(note: Note) -> Option<u8> {
    let note_u8 = note as u8;
    match note_u8.div_rem(&8) {
        (0, n) => Some(n),
        _ => None,
    }
}


impl InnerState {
    fn set_modulated(&mut self, p: Param, root_value: u8) {
        self.mod_dump.insert(p, root_value);
    }

    fn unset_modulated(&mut self, p: Param, context: &mut RouteContext) -> Result<(), MidiError> {
        if let Some(root) = self.mod_dump.remove(&p) {
            if let Some(dump) = &mut self.current_dump {
                set_param_value(p, root, dump.as_mut_slice());
                self.send_param_value(p, context)?;
            }
        }
        Ok(())
    }

    fn send_param_value(&mut self, param: Param, context: &mut RouteContext) -> Result<(), MidiError> {
        if let Some(dump) = &mut self.current_dump {
            context.packets.clear();
            context.packets.extend(param_to_sysex(param, &dump))
        }
        Ok(())
    }
}

impl Service for Dw6000Control {
    fn start(&mut self, now: rtic::cyccnt::Instant, router: &mut Router, tasks: &mut Tasks) -> Result<(), MidiError> {
        let state = self.state.clone();
        tasks.repeat(now, move |_now, chaos, spawn| {
            let mut state = state.lock();

            // LFO2 modulation
            if let Some(lfo2_param) = state.lfo2_param.map(|p| Param::from(p)) {
                if let Some(root) = state.mod_dump.get(&lfo2_param).cloned() {
                    let max = lfo2_param.max_value();
                    let fmax = max as f32;
                    let froot: f32 = root as f32 / fmax;

                    let fmod = state.lfo2.mod_value(froot, long_now(), chaos) * fmax;
                    let mod_value = fmod.max(0.0).min(fmax) as u8;

                    if let Some(ref mut dump) = &mut state.current_dump {
                        set_param_value(lfo2_param, mod_value, dump.as_mut_slice());
                        let sysex = param_to_sysex(lfo2_param, &dump);
                        spawn.midispatch(Dst(state.dw6000.interface), sysex.collect())?;
                    }
                }
            }
            Ok(Some(50.millis()))
        });

        let state = self.state.clone();
        tasks.repeat(now, move |_now, _chaos, spawn| {
            let state = state.lock();
            // periodic DW-6000 dump sync
            spawn.midispatch(Dst(state.dw6000.interface), dump_request().collect())?;
            Ok(Some(250.millis()))
        });

        let beatstep = self.state.lock().beatstep;
        let dw6000 = self.state.lock().dw6000;

        // handle messages from controller
        let state = self.state.clone();
        router.add_route(
            Route::link(beatstep.interface, dw6000.interface)
                .filter(move |_now, context| {
                    let mut state = state.lock();
                    for p in context.packets.clone().iter() {
                        if let Ok(msg) = Message::try_from(*p) {
                            from_beatstep(dw6000, msg, &mut state, context)?;
                        }
                    }
                    Ok(true)
                }))?;

        // handle messages from dw6000
        let state = self.state.clone();
        router.add_route(
            Route::from(dw6000.interface)
                .filter({
                    let mut matcher = dump_matcher();
                    move |_now, context| capture_sysex(&mut matcher, context)
                })
                .filter(
                    move |_now, context| {
                        let state = state.lock();
                        from_dw6000_dump(context, state)
                    }
                ))?;

        rprintln!("DW6000 Controller Active");
        Ok(())
    }
}

fn toggle_param(param: Param, dump: &mut Vec<u8>, context: &mut RouteContext) -> Result<(), MidiError> {
    let mut value = get_param_value(param, dump.as_slice());
    value = value ^ 1;
    set_param_value(param, value, dump.as_mut_slice());
    context.packets.clear();
    for packet in param_to_sysex(param, dump.as_slice()) {
        context.packets.push(packet);
    }
    context.strings.push(format!("{:?}\n{:.2}", param, value));
    Ok(())
}

fn from_beatstep(dw6000: Endpoint, msg: Message, state: &mut MutexGuard<InnerState>, context: &mut RouteContext) -> Result<bool, MidiError> {
    match msg {
        Message::NoteOn(_, note, _) => {
            if let Some(bank) = note_bank(note) {
                state.bank = Some(bank)
            } else if let Some(prog) = note_prog(note) {
                if let Some(bank) = state.bank {
                    // spawn.midisend(dw6000.interface, .into())?;
                    context.packets.clear();
                    context.packets.push(program_change(dw6000.channel, (bank * 8) + prog)?.into());
                }
            }
            if let Some(page) = note_page(note) {
                state.temp_page = Some((page, long_now()));
            }
            if let Some(tog) = toggle_page(note) {
                if let Some(dump) = &mut state.current_dump {
                    match tog {
                        TogglePage::Arp => {}
                        TogglePage::Latch => {}
                        TogglePage::Polarity => toggle_param(Param::Polarity, dump, context)?,
                        TogglePage::Chorus => toggle_param(Param::Chorus, dump, context)?,
                    }
                }
            }
        }
        Message::NoteOff(_, note, _) => {
            if state.bank == note_bank(note) {
                state.bank = None
            }
            if let Some((temp_page, press_time)) = state.temp_page {
                if let Some(note_page) = note_page(note) {
                    if note_page == temp_page {
                        let held_for: BigDuration = long_now() - press_time;
                        if held_for.millis() < SHORT_PRESS {
                            state.base_page = temp_page;
                        }
                        state.temp_page = None;
                    }
                }
            }
        }
        Message::ControlChange(_ch, cc, value) =>
            if let Some(param) = cc_to_dw_param(cc, state.active_page()) {
                if let Some(root) = state.mod_dump.get_mut(&param) {
                    *root = value.0
                } else if let Some(dump) = &mut state.current_dump {
                    set_param_value(param, value.into(), dump.as_mut_slice());
                    context.packets.clear();
                    context.packets.extend(param_to_sysex(param, &dump));
                    context.strings.push(format!("{:?}\n{:?}", param, get_param_value(param, &dump)));
                } else {
                    rprintln!("no dump yet");
                }
            } else if let Some(param) = cc_to_ctl_param(cc, state.active_page()) {
                match param {
                    CtlParam::Lfo2Rate => {
                        let base_rate = (value.0 as f32 + 1.0) * 0.1;
                        rprintln!("ratev {} ratex {}", value.0, base_rate);
                        state.lfo2.set_rate_hz(base_rate.min(40.0).max(0.03));
                        context.strings.push(format!("{:?}\n{:.2}", param, state.lfo2.get_rate_hz()));
                    }
                    CtlParam::Lfo2Amt => {
                        state.lfo2.set_amount(f32::from(value.0) / f32::from(U7::MAX.0));
                        context.strings.push(format!("{:?}\n{:.2}", param, state.lfo2.get_amount()));
                    }
                    CtlParam::Lfo2Wave => {
                        state.lfo2.set_waveform(Waveform::from(value.0.min(3)));
                        context.strings.push(format!("{:?}\n{:?}", param, state.lfo2.get_waveform()));
                    }
                    CtlParam::Lfo2Dest => {
                        if let Some(mod_p) = state.lfo2_param.map(|p| Param::from(p)) {
                            state.unset_modulated(mod_p, context)?;
                        }
                        if let Some(ref mut dump) = &mut state.current_dump {
                            let new_dest = Lfo2Dest::try_from(value.0).ok();
                            if let Some(mod_p) = new_dest.map(|p| Param::from(p)) {
                                let saved_val = get_param_value(mod_p, &dump);
                                state.set_modulated(mod_p, saved_val);
                                state.lfo2_param = new_dest;
                                context.strings.push(format!("{:?}\n{:?}", param, mod_p));
                            }
                        }
                    }
                }
                rprintln!("lfo {:?}", &state.lfo2)
            }
        _ => {}
    }
    Ok(true)
}

#[derive(Debug, Copy, Clone)]
enum CtlParam {
    Lfo2Rate,
    Lfo2Wave,
    Lfo2Dest,
    Lfo2Amt,
}

fn cc_to_ctl_param(cc: midi::Control, page: KnobPage) -> Option<CtlParam> {
    match page {
        KnobPage::Mod => {
            match cc.into() {
                9 => Some(CtlParam::Lfo2Rate),
                10 => Some(CtlParam::Lfo2Amt),
                11 => Some(CtlParam::Lfo2Wave),
                12 => Some(CtlParam::Lfo2Dest),
                _ => None
            }
        }
        // KnobPage::Arp => {}
        _ => None
    }
}

fn from_dw6000_dump(ctx: &mut RouteContext, mut state: MutexGuard<InnerState>) -> Result<bool, MidiError> {
    if let Some(mut dump) = ctx.tags.remove(&Tag::Dump(26)) {
        // rewrite original values before they were modulated
        for s in &state.mod_dump {
            set_param_value(*s.0, *s.1, &mut dump)
        }
        state.current_dump = Some(dump);
    }
    Ok(false)
}

fn param_to_sysex(param: Param, dump_buf: &[u8]) -> sysex::Sysex {
    let dump = dw6000::as_dump_ref(dump_buf);
    match param {
        Param::AssignMode | Param::BendOsc => parameter_set(0, dump.assign_mode_bend_osc.0),
        Param::Portamento => parameter_set(1, dump.portamento_time.0),
        Param::Osc1Level => parameter_set(2, dump.osc1_level.0),
        Param::Osc2Level => parameter_set(3, dump.osc2_level.0),
        Param::Noise => parameter_set(4, dump.noise_level.0),
        Param::Cutoff => parameter_set(5, dump.cutoff.0),
        Param::Resonance => parameter_set(6, dump.resonance.0),
        Param::VcfInt => parameter_set(7, dump.vcf_eg_int.0),
        Param::VcfAttack => parameter_set(8, dump.vcf_eg_attack.0),
        Param::VcfDecay => parameter_set(9, dump.vcf_eg_decay.0),
        Param::VcfBreak => parameter_set(10, dump.vcf_eg_breakpoint.0),
        Param::VcfSlope => parameter_set(11, dump.vcf_eg_slope.0),
        Param::VcfSustain => parameter_set(12, dump.vcf_eg_sustain.0),
        Param::VcfRelease => parameter_set(13, dump.vcf_eg_release.0),
        Param::VcaAttack => parameter_set(14, dump.vca_eg_attack.0),
        Param::VcaDecay => parameter_set(15, dump.vca_eg_decay.0),
        Param::VcaBreak => parameter_set(16, dump.vca_eg_breakpoint.0),
        Param::VcaSlope => parameter_set(17, dump.vca_eg_slope.0),
        Param::BendVcf | Param::VcaSustain => parameter_set(18, dump.bend_vcf_vca_eg_sustain.0),
        Param::Osc1Octave | Param::VcaRelease => parameter_set(19, dump.osc1_oct_vca_eg_release.0),
        Param::Osc2Octave | Param::MgFreq => parameter_set(20, dump.osc2_oct_mg_freq.0),
        Param::KbdTrack | Param::MgDelay => parameter_set(21, dump.kbd_track_mg_delay.0),
        Param::Polarity | Param::MgOsc => parameter_set(22, dump.polarity_mg_osc.0),
        Param::Chorus | Param::MgVcf => parameter_set(23, dump.chorus_mg_vcf.0),
        Param::Osc1Wave | Param::Osc2Wave => parameter_set(24, dump.osc1_wave_osc2_wave.0),
        Param::Osc2Detune | Param::Interval => parameter_set(25, dump.osc2_interval_osc2_detune.0),
    }
}

fn cc_to_dw_param(cc: midi::Control, page: KnobPage) -> Option<Param> {
    match cc.into() {
        // jogwheel hardwired to cutoff for her pleasure
        17 => return Some(Param::Cutoff),
        8 => return Some(Param::Resonance),
        // AssignMode => defined on DW6000 panel
        18 => return Some(Param::Polarity),
        19 => return Some(Param::Chorus),

        _ => {}
    }

    match page {
        KnobPage::Osc => match cc.into() {
            1 => Some(Param::Osc1Level),
            2 => Some(Param::Osc1Octave),
            3 => Some(Param::Osc1Wave),
            4 => Some(Param::Noise),
            5 => Some(Param::BendOsc),
            6 => Some(Param::BendVcf),
            7 => Some(Param::Portamento),

            9 => Some(Param::Osc2Level),
            10 => Some(Param::Osc2Octave),
            11 => Some(Param::Osc2Wave),
            12 => Some(Param::Interval),
            13 => Some(Param::Osc2Detune),
            14 => Some(Param::Osc2Wave),
            _ => None
        }
        KnobPage::Env => match cc.into() {
            1 => Some(Param::VcaAttack),
            2 => Some(Param::VcaDecay),
            3 => Some(Param::VcaBreak),
            4 => Some(Param::VcaSustain),
            5 => Some(Param::VcaSlope),
            6 => Some(Param::VcaRelease),

            9 => Some(Param::VcfAttack),
            10 => Some(Param::VcfDecay),
            11 => Some(Param::VcfBreak),
            12 => Some(Param::VcfSustain),
            13 => Some(Param::VcfSlope),
            14 => Some(Param::VcfRelease),
            15 => Some(Param::VcfInt),
            16 => Some(Param::KbdTrack),
            _ => None,
        }
        KnobPage::Mod => match cc.into() {
            1 => Some(Param::MgFreq),
            2 => Some(Param::MgDelay),
            3 => Some(Param::MgOsc),
            4 => Some(Param::MgVcf),
            5 => Some(Param::BendOsc),
            6 => Some(Param::BendVcf),
            7 => Some(Param::Portamento),
            // TODO LFO2 (? - Rate, Sync, Shape, Amt, Target)
            _ => None,
        }
        KnobPage::Arp => match cc.into() {
            // TODO Arp control (Rate, Oct, Mode, Order)
            0 => None,
            _ => None,
        }
    }
}
