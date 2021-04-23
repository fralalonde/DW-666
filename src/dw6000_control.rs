use crate::midi::{Interface, Channel, RouteId, Router, Route, capture_sysex, Service, Message, Note, RouterEvent, Tag, Handler, RouteContext, program_change, MidiError};
use crate::{devices, clock, midi};
use alloc::vec::Vec;
use alloc::sync::Arc;
use core::convert::TryFrom;
use crate::clock::{Instant, Duration};
use clock::long_now;
use devices::korg::dw6000::*;
use spin::MutexGuard;
use num_enum::TryFromPrimitive;
use num::Integer;

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
    dw6000: Endpoint,
    beatstep: Endpoint,
    routes: Vec<RouteId>,
}

impl Dw6000Control {
    pub fn new(dw6000: impl Into<Endpoint>, beatstep: impl Into<Endpoint>) -> Self {
        Dw6000Control {
            dw6000: dw6000.into(),
            beatstep: beatstep.into(),
            routes: vec![],
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
enum KnobPage {
    Osc,
    Env,
    Mod,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ProgPage {
    Lo(u8),
    Hi(u8),
}

#[derive(Debug)]
enum ArpMode {
    Up,
    Down,
    UpDown,
}

#[derive(Debug, Clone)]
struct State {
    inner: Arc<spin::Mutex<InnerState>>
}

#[derive(Debug)]
struct InnerState {
    current_dump: Option<(Vec<u8>, Instant)>,
    base_page: KnobPage,
    // if temp_page is released quickly, is becomes base_page
    temp_page: Option<(KnobPage, Instant)>,
    bank: Option<u8>,
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

impl Service for Dw6000Control {
    fn start(&mut self, router: &mut Router) {
        let state: State = State {
            inner: Arc::new(spin::Mutex::new(InnerState {
                current_dump: None,
                base_page: KnobPage::Osc,
                temp_page: None,
                bank: None,
            }))
        };

        // FROM BEATSTEP
        let page_state: State = state.clone();
        let dw6000 = self.dw6000;
        let page_if = router.add_interface(Handler::new(move |_now, event, _ctx, spawn, _sched| {
            if let RouterEvent::Packet(packet) = event {
                if let Ok(msg) = Message::try_from(packet) {
                    let state = page_state.inner.lock();
                    if let Err(e) = from_beatstep(dw6000, msg, spawn, state) {
                        rprintln!("Error from Beatstep {:?}", e);
                    }
                }
            }
        }));

        self.routes.push(router.bind(
            Route::link(self.beatstep.interface, page_if)
        ));

        // FROM DW-6000
        let dump_if = router.add_interface(Handler::new(move |_now, _event, ctx, _spawn, _sched| {
            let state = state.inner.lock();
            from_dw6000(ctx, state)
        }));

        self.routes.push(router.bind(
            Route::link(self.dw6000.interface, dump_if)
                .filter(capture_sysex(dump_matcher()))
        ));

        rprintln!("DW6000 Controller Active")
    }

    fn stop(&mut self, router: &mut Router) {
        todo!()
    }
}

fn from_beatstep(dw6000: Endpoint, msg: Message, spawn: crate::dispatch_from::Spawn, mut state: MutexGuard<InnerState>) -> Result<(), MidiError> {
    match msg {
        Message::NoteOn(_, note, _) => {
            if let Some(bank) = note_bank(note) {
                state.bank = Some(bank)
            } else if let Some(prog) = note_prog(note) {
                if let Some(bank) = state.bank {
                    rprintln!("bank {:x?} prog {:x?} patch {:x?}", bank as u8, prog as u8, (bank * 8) + prog);
                    spawn.send_midi(dw6000.interface, program_change(dw6000.channel, (bank * 8) + prog)?.into());
                    return Ok(())
                }
            }
            if let Some(page) = note_page(note) {
                state.temp_page = Some((page, long_now()));
            }
        }
        Message::NoteOff(_, note, _) => {
            if state.bank == note_bank(note) {
                state.bank = None
            }
            if let Some((temp_page, press_time)) = state.temp_page {
                if let Some(note_page) = note_page(note) {
                    if note_page == temp_page {
                        let held_for: Duration = long_now() - press_time;
                        if held_for.millis() < SHORT_PRESS {
                            state.base_page = temp_page;
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
                } else {
                    // TODO init dump eagerly, then keep it synced
                    for packet in dump_request() {
                        spawn.send_midi(dw6000.interface, packet).unwrap();
                    }
                }
            }
        _ => {}
    }
    Ok(())
}

fn from_dw6000(ctx: RouteContext, mut state: MutexGuard<InnerState>) {
    if let Some(dump) = ctx.tags.get(&Tag::Dump(26)) {
        let long_now = long_now();
        state.current_dump = Some((dump.clone(), long_now));
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

            // TODO Arp control (Rate, Oct, Mode, Order)
            // TODO LFO2 (? - Rate, Sync, Shape, Amt, Target)
            _ => None,
        }
    }
}
