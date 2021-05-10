//! Sends MIDI to Korg DW-6000 acccording to messages
//!
use crate::midi::{Interface, Channel, Router, Route, capture_sysex, Service, Message, Note, RouterEvent, Tag, Handler, RouteContext, program_change, MidiError, Sysex, U7};
use crate::{devices, clock, midi, Handle};
use alloc::vec::Vec;
use alloc::sync::Arc;
use core::convert::TryFrom;
use crate::clock::{BigInstant, TimeUnits, BigDuration};
use clock::long_now;
use devices::korg::dw6000::*;
use spin::MutexGuard;
use num_enum::TryFromPrimitive;
use num::Integer;
use alloc::boxed::Box;
use crate::apps::lfo::{Lfo, Waveform};
use rtic::cyccnt::U32Ext;
use hashbrown::HashMap;
use crate::devices::korg::dw6000;

const SHORT_PRESS: u32 = 250;

#[derive(Copy, Clone, Debug)]
pub struct Endpoint {
    interface: Interface,
    channel: Channel,
}

impl From<(Interface, Channel)> for Endpoint {
    fn from(pa: (Interface, Channel)) -> Self {
        Endpoint { interface: pa.0, channel: pa.1 }
    }
}

pub struct Dw6000Control {
    routes: Vec<Handle>,
    state: Arc<spin::Mutex<InnerState>>,
}

impl Dw6000Control {
    pub fn new(dw6000: impl Into<Endpoint>, beatstep: impl Into<Endpoint>) -> Self {
        Dw6000Control {
            routes: vec![],
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ProgPage {
    Lo(u8),
    Hi(u8),
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

    fn unset_modulated(&mut self, p: Param, spawn: crate::dispatch_from::Spawn) {
        if let Some(root) = self.mod_dump.remove(&p) {
            if let Some(dump) = &mut self.current_dump {
                set_param_value(p, root, dump.as_mut_slice());
                self.send_param_value(p, spawn);
            }
        }
    }

    fn send_param_value(&mut self, param: Param, spawn: crate::dispatch_from::Spawn) {
        if let Some(dump) = &mut self.current_dump {
            for packet in param_to_sysex(param, &dump) {
                spawn.send_midi(self.dw6000.interface, packet).unwrap();
            }
        }
    }
}

impl Service for Dw6000Control {
    fn start(&mut self, now: rtic::cyccnt::Instant, router: &mut Router, schedule: crate::init::Schedule) {

        // periodic DW-6000 dump request
        let state = self.state.clone();
        schedule.timer_task(now + 0.cycles(), Box::new(move |_resources, spawn| {
            let state = state.lock();
            for packet in dump_request() {
                spawn.send_midi(state.dw6000.interface, packet).unwrap();
            }
            Some(100.millis())
        }));

        // periodic LFO2 modulation
        let state = self.state.clone();
        schedule.timer_task(now + 0.cycles(), Box::new(move |resources, spawn| {
            let mut state = state.lock();
            if let Some(lfo2_param) = state.lfo2_param.map(|p| Param::from(p)) {
                if let Some(root) = state.mod_dump.get(&lfo2_param).cloned() {
                    let mod_value = state.lfo2.mod_value(root, long_now(), resources.chaos);
                    if let Some(ref mut dump) = &mut state.current_dump {
                        set_param_value(lfo2_param, mod_value, dump.as_mut_slice());
                        for packet in param_to_sysex(lfo2_param, &dump) {
                            spawn.send_midi(state.dw6000.interface, packet).unwrap();
                        }
                    }
                }
            }
            Some(state.lfo2.next_iter())
        }));

        // handle messages from controller
        let state = self.state.clone();
        let page_if = router.add_interface(Handler::new(move |_now, event, _ctx, spawn, _sched| {
            if let RouterEvent::Packet(packet) = event {
                if let Ok(msg) = Message::try_from(packet) {
                    let state = state.lock();
                    if let Err(e) = from_beatstep(state.dw6000, msg, spawn, state) {
                        rprintln!("Error from Beatstep {:?}", e);
                    }
                }
            }
        }));

        // handle messages from dw6000
        let state = self.state.clone();
        let dump_if = router.add_interface(Handler::new(move |_now, _event, ctx, _spawn, _sched| {
            let state = state.lock();
            from_dw6000_dump(ctx, state)
        }));

        let state = self.state.lock();

        self.routes.push(router.bind(
            Route::link(state.beatstep.interface, page_if)
        ));

        self.routes.push(router.bind(
            Route::link(state.dw6000.interface, dump_if)
                .filter(capture_sysex(dump_matcher()))
        ));

        rprintln!("DW6000 Controller Active")
    }
}

fn toggle_param(param: Param, dump: &mut Vec<u8>, dw6000: Endpoint, spawn: crate::dispatch_from::Spawn) {
    let mut value = get_param_value(param, dump.as_slice());
    value = value ^ 1;
    set_param_value(param, value, dump.as_mut_slice());
    for packet in param_to_sysex(param, dump.as_slice()) {
        spawn.send_midi(dw6000.interface, packet).unwrap();
    }
}

fn from_beatstep(dw6000: Endpoint, msg: Message, spawn: crate::dispatch_from::Spawn, mut state: MutexGuard<InnerState>) -> Result<(), MidiError> {
    match msg {
        Message::NoteOn(_, note, _) => {
            if let Some(bank) = note_bank(note) {
                state.bank = Some(bank)
            } else if let Some(prog) = note_prog(note) {
                if let Some(bank) = state.bank {
                    spawn.send_midi(dw6000.interface, program_change(dw6000.channel, (bank * 8) + prog)?.into());
                    return Ok(());
                }
            }
            if let Some(page) = note_page(note) {
                state.temp_page = Some((page, long_now()));
            }
            if let Some(tog) = toggle_page(note) {
                if let Some(dump) = &mut state.current_dump {
                    match tog {
                        TogglePage::Arp => { todo!("add inner switches state") }
                        TogglePage::Latch => { todo!("add inner switches state") }
                        TogglePage::Polarity => toggle_param(Param::Polarity, dump, dw6000, spawn),
                        TogglePage::Chorus => toggle_param(Param::Chorus, dump, dw6000, spawn),
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
                if let Some(dump) = &mut state.current_dump {
                    set_param_value(param, value.into(), dump.as_mut_slice());
                    for packet in param_to_sysex(param, &dump) {
                        spawn.send_midi(dw6000.interface, packet).unwrap();
                    }
                    spawn.redraw(format!("{:?}\n{:?}", param, get_param_value(param, &dump)));
                }
            } else if let Some(param) = cc_to_ctl_param(cc, state.active_page()) {
                match param {
                    CtlParam::Lfo2Rate => {
                        let base_rate = f32::from(value.0) + 1.01;
                        state.lfo2.set_rate_hz(base_rate/*.exp()*/);
                        spawn.redraw(format!("{:?}\n{:?}", param, state.lfo2.get_rate_hz()));
                    }
                    CtlParam::Lfo2Amt => {
                        rprintln!("amt {:?} -> {:?}", value.0, f32::from(value.0) / f32::from(U7::MAX.0));
                        state.lfo2.set_amount(f32::from(value.0) / f32::from(U7::MAX.0));
                        spawn.redraw(format!("{:?}\n{:?}", param, state.lfo2.get_amount()));
                    }
                    CtlParam::Lfo2Wave => {
                        state.lfo2.set_waveform(Waveform::from(value.0.max(3)));
                        spawn.redraw(format!("{:?}\n{:?}", param, state.lfo2.get_waveform()));
                    }
                    CtlParam::Lfo2Dest => {
                        if let Some(mod_p) = state.lfo2_param.map(|p| Param::from(p)) {
                            state.unset_modulated(mod_p, spawn);
                        }
                        if let Some(ref mut dump) = &mut state.current_dump {
                            let new_dest = Lfo2Dest::try_from(value.0).ok();
                            if let Some(mod_p) = new_dest.map(|p| Param::from(p)) {
                                let saved_val = get_param_value(mod_p, &dump);
                                state.set_modulated(mod_p, saved_val);
                            }
                            state.lfo2_param = new_dest;
                            spawn.redraw(format!("{:?}\n{:?}", param, state.lfo2_param));
                        }
                    }
                }
            }
        _ => {}
    }
    Ok(())
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

fn from_dw6000_dump(mut ctx: RouteContext, mut state: MutexGuard<InnerState>) {
    if let Some(mut dump) = ctx.tags.remove(&Tag::Dump(26)) {
        // rewrite original values before they were modulated
        for s in &state.mod_dump {
            set_param_value(*s.0, *s.1, &mut dump)
        }
        state.current_dump = Some(dump);
    }
}

fn param_to_sysex(param: Param, dump_buf: &[u8]) -> Sysex {
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