//! Sends MIDI to Korg DW-6000 acccording to messages
//!
use crate::midi::{Interface, Channel, Router, Route, capture_sysex, Service, Message, Note, RouterEvent, Tag, Handler, RouteContext, program_change, MidiError, Sysex};
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
use crate::apps::lfo::Lfo;
use rtic::cyccnt::U32Ext;
use hashbrown::HashMap;
use crate::devices::korg::dw6000;

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
                lfo2_param: Lfo2Dest::None,
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

#[derive(Debug, Copy, Clone)]
enum Lfo2Dest {
    None,
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
    MgFreq,
    MgDelay,
    MgOsc,
    MgVcf,

}


impl TryFrom<Lfo2Dest> for Param {
    type Error = MidiError;

    fn try_from(dest: Lfo2Dest) -> Result<Self, MidiError> {
        match dest {
            Lfo2Dest::None => Err(MidiError::NoModeForParameter),
            Lfo2Dest::Osc1Wave => Ok(Param::Osc1Wave),
            Lfo2Dest::Osc1Level => Ok(Param::Osc1Level),
            Lfo2Dest::Osc1Octave => Ok(Param::Osc1Octave),
            Lfo2Dest::Osc2Wave => Ok(Param::Osc2Wave),
            Lfo2Dest::Osc2Level => Ok(Param::Osc2Level),
            Lfo2Dest::Osc2Octave => Ok(Param::Osc2Octave),
            Lfo2Dest::Osc2Detune => Ok(Param::Osc2Detune),
            Lfo2Dest::Osc2Interval => Ok(Param::Osc2Interval),
            Lfo2Dest::NoiseLevel => Ok(Param::NoiseLevel),
            Lfo2Dest::Cutoff => Ok(Param::Cutoff),
            Lfo2Dest::Resonance => Ok(Param::Resonance),
            Lfo2Dest::VcfEgInt => Ok(Param::VcfEgInt),
            Lfo2Dest::VcfEgAttack => Ok(Param::VcfEgAttack),
            Lfo2Dest::VcfEgDecay => Ok(Param::VcfEgDecay),
            Lfo2Dest::VcfEgBreakpoint => Ok(Param::VcfEgBreakpoint),
            Lfo2Dest::VcfEgSlope => Ok(Param::VcfEgSlope),
            Lfo2Dest::VcfEgSustain => Ok(Param::VcfEgSustain),
            Lfo2Dest::VcfEgRelease => Ok(Param::VcfEgRelease),
            Lfo2Dest::VcaEgAttack => Ok(Param::VcaEgAttack),
            Lfo2Dest::VcaEgDecay => Ok(Param::VcaEgDecay),
            Lfo2Dest::VcaEgBreakpoint => Ok(Param::VcaEgBreakpoint),
            Lfo2Dest::VcaEgSlope => Ok(Param::VcaEgSlope),
            Lfo2Dest::VcaEgSustain => Ok(Param::VcaEgSustain),
            Lfo2Dest::VcaEgRelease => Ok(Param::VcaEgRelease),
            Lfo2Dest::MgFreq => Ok(Param::MgFreq),
            Lfo2Dest::MgDelay => Ok(Param::MgDelay),
            Lfo2Dest::MgOsc => Ok(Param::MgOsc),
            Lfo2Dest::MgVcf => Ok(Param::MgVcf),
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

    current_dump: Option<(Vec<u8>, BigInstant)>,
    // saved values from dump before being modulated
    mod_dump: HashMap<Param, u8>,
    base_page: KnobPage,
    // if temp_page is released quickly, is becomes base_page
    temp_page: Option<(KnobPage, BigInstant)>,
    bank: Option<u8>,
    lfo2: Lfo,
    lfo2_param: Lfo2Dest,
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

const SHORT_PRESS: u32 = 250;

const MAX_DUMP_AGE: u32 = 250;

impl InnerState {
    fn set_modulated(&mut self, p: Param, root_value: u8) {
        self.mod_dump.insert(p, root_value);
    }

    fn unset_mmodulated(&mut self, p: Param, spawn: crate::dispatch_from::Spawn) {
        if let Some(root) = self.mod_dump.remove(&p) {
            if let Some(dump) = &mut self.current_dump {
                set_param_value(p, root, dump.0.as_mut_slice());
                self.send_param_value(p, spawn);
            }
        }
    }

    fn send_param_value(&mut self, param: Param, spawn: crate::dispatch_from::Spawn) {
        if let Some(dump) = &mut self.current_dump {
            for packet in param_to_sysex(param, dump.0.as_slice()) {
                spawn.send_midi(self.dw6000.interface, packet).unwrap();
            }
        }
    }
}

impl Service for Dw6000Control {
    fn start(&mut self, now: rtic::cyccnt::Instant, router: &mut Router, schedule: crate::init::Schedule) {
        let state = self.state.clone();
        schedule.timer_task(now + 0.cycles(), Box::new(move |_resources, spawn| {
            let state = state.lock();
            for packet in dump_request() {
                spawn.send_midi(state.dw6000.interface, packet).unwrap();
            }
            Some(100.millis())
        }));

        let state = self.state.clone();
        schedule.timer_task(now + 0.cycles(), Box::new(move |resources, spawn| {
            let mut state = state.lock();
            if let Ok(lfo2_param) = Param::try_from(state.lfo2_param) {
                let value = state.lfo2.update_value(long_now(), resources.chaos);
                if let Some((dump, ref mut time)) = &mut state.current_dump {
                    set_param_value(lfo2_param, (value >> 25) as u8, dump.as_mut_slice());
                    *time = long_now();
                    for packet in param_to_sysex(lfo2_param, dump.as_slice()) {
                        spawn.send_midi(state.dw6000.interface, packet).unwrap();
                    }
                }
            }
            Some(state.lfo2.next_iter())
        }));

        // FROM BEATSTEP
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

        // FROM DW-6000
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
                if let Some((dump, ref mut _time)) = &mut state.current_dump {
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
                            rprintln!("short press {:x?}", held_for.millis());
                            state.base_page = temp_page;
                        } else {
                            rprintln!("long press {:x?}", held_for.millis());
                        }
                        state.temp_page = None;
                    }
                }
            }
        }
        Message::ControlChange(_ch, cc, value) =>
            if let Some(param) = cc_to_param(cc, state.active_page()) {
                if let Some((dump, ref mut time)) = &mut state.current_dump {
                    set_param_value(param, value.into(), dump.as_mut_slice());
                    *time = long_now();
                    for packet in param_to_sysex(param, dump.as_slice()) {
                        spawn.send_midi(dw6000.interface, packet).unwrap();
                    }
                }
            }
        _ => {}
    }
    Ok(())
}

fn from_dw6000_dump(mut ctx: RouteContext, mut state: MutexGuard<InnerState>) {
    if let Some(dump) = ctx.tags.get_mut(&Tag::Dump(26)) {
        let long_now = long_now();
        // rewrite original values before they were modulated
        for s in &state.mod_dump {
            set_param_value(*s.0, *s.1, dump)
        }
        state.current_dump = Some((dump.clone(), long_now));
    }
}

fn param_to_sysex(param: Param, dump_buf: &[u8]) -> Sysex {
    let dump = dw6000::as_dump_ref(dump_buf);
    match param {
        Param::AssignMode | Param::BendOsc => parameter_set(0, dump.assign_mode_bend_osc.0),
        Param::PortamentoTime => parameter_set(1, dump.portamento_time.0),
        Param::Osc1Level => parameter_set(2, dump.osc1_level.0),
        Param::Osc2Level => parameter_set(3, dump.osc2_level.0),
        Param::NoiseLevel => parameter_set(4, dump.noise_level.0),
        Param::Cutoff => parameter_set(5, dump.cutoff.0),
        Param::Resonance => parameter_set(6, dump.resonance.0),
        Param::VcfEgInt => parameter_set(7, dump.vcf_eg_int.0),
        Param::VcfEgAttack => parameter_set(8, dump.vcf_eg_attack.0),
        Param::VcfEgDecay => parameter_set(9, dump.vcf_eg_decay.0),
        Param::VcfEgBreakpoint => parameter_set(10, dump.vcf_eg_breakpoint.0),
        Param::VcfEgSlope => parameter_set(11, dump.vcf_eg_slope.0),
        Param::VcfEgSustain => parameter_set(12, dump.vcf_eg_sustain.0),
        Param::VcfEgRelease => parameter_set(13, dump.vcf_eg_release.0),
        Param::VcaEgAttack => parameter_set(14, dump.vca_eg_attack.0),
        Param::VcaEgDecay => parameter_set(15, dump.vca_eg_decay.0),
        Param::VcaEgBreakpoint => parameter_set(16, dump.vca_eg_breakpoint.0),
        Param::VcaEgSlope => parameter_set(17, dump.vca_eg_slope.0),
        Param::BendVcf | Param::VcaEgSustain => parameter_set(18, dump.bend_vcf_vca_eg_sustain.0),
        Param::Osc1Octave | Param::VcaEgRelease => parameter_set(19, dump.osc1_oct_vca_eg_release.0),
        Param::Osc2Octave | Param::MgFreq => parameter_set(20, dump.osc2_oct_mg_freq.0),
        Param::KbdTrack | Param::MgDelay => parameter_set(21, dump.kbd_track_mg_delay.0),
        Param::Polarity | Param::MgOsc => parameter_set(22, dump.polarity_mg_osc.0),
        Param::Chorus | Param::MgVcf => parameter_set(23, dump.chorus_mg_vcf.0),
        Param::Osc1Wave | Param::Osc2Wave => parameter_set(24, dump.osc1_wave_osc2_wave.0),
        Param::Osc2Detune | Param::Osc2Interval => parameter_set(25, dump.osc2_interval_osc2_detune.0),
    }
}

fn cc_to_param(cc: midi::Control, page: KnobPage) -> Option<Param> {
    match cc.into() {
        // jogwheel is hardwired to cutoff for her pleasure
        17 => return Some(Param::Cutoff),
        8 => return Some(Param::Resonance),
        // AssignMode => defined on DW6000 panel

        // TODO weird switch... maybe use synthetic signed VcfEgInt (-32..32) instead? but quick toggling could lead to interesting effects
        18 => return Some(Param::Polarity),
        19 => return Some(Param::Chorus),

        _ => {}
    }

    match page {
        KnobPage::Osc => match cc.into() {
            1 => Some(Param::Osc1Level),
            2 => Some(Param::Osc1Octave),
            3 => Some(Param::Osc1Wave),
            4 => Some(Param::NoiseLevel),
            5 => Some(Param::BendOsc),
            6 => Some(Param::BendVcf),
            7 => Some(Param::PortamentoTime),

            9 => Some(Param::Osc2Level),
            10 => Some(Param::Osc2Octave),
            11 => Some(Param::Osc2Wave),
            12 => Some(Param::Osc2Interval),
            13 => Some(Param::Osc2Detune),
            14 => Some(Param::Osc2Wave),
            _ => None
        }
        KnobPage::Env => match cc.into() {
            1 => Some(Param::VcaEgAttack),
            2 => Some(Param::VcaEgDecay),
            3 => Some(Param::VcaEgBreakpoint),
            4 => Some(Param::VcaEgSustain),
            5 => Some(Param::VcaEgSlope),
            6 => Some(Param::VcaEgRelease),

            9 => Some(Param::VcfEgAttack),
            10 => Some(Param::VcfEgDecay),
            11 => Some(Param::VcfEgBreakpoint),
            12 => Some(Param::VcfEgSustain),
            13 => Some(Param::VcfEgSlope),
            14 => Some(Param::VcfEgRelease),
            15 => Some(Param::VcfEgInt),
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
            7 => Some(Param::PortamentoTime),
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
