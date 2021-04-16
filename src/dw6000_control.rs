use crate::midi::{Interface, Channel, RouteId, Router, Route, capture_sysex, Service, Message, Note, RouterEvent, Tag, Handler};
use crate::{devices, clock};
use alloc::vec::Vec;
use devices::korg_dw6000;
use alloc::sync::Arc;
use core::convert::TryFrom;
use crate::clock::{Instant, Duration};
use clock::long_now;
use crate::devices::korg_dw6000::Program;

pub struct Endpoint {
    interface: Interface,
    channel: Channel,
}


pub struct Dw6000Control {
    dw6000: Endpoint,
    beatstep: Endpoint,
    routes: Vec<RouteId>,
}

impl Dw6000Control {
    pub fn new(dw6000: Endpoint, beatstep: Endpoint) -> Self {
        Dw6000Control {
            dw6000,
            beatstep,
            routes: vec![],
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Page {
    Osc,
    Vcf,
    Vca,
    Mod,
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
    current_program: Option<(korg_dw6000::Program, Instant)>,
    base_page: Page,
    // if temp_page is released quickly, is becomes base_page
    temp_page: Option<(Page, Instant)>,
    // arp_enabled: bool,
    // arp_mode: ArpMode,
    // arp_oct: u8, // 1..4
}

impl InnerState {
    fn active_page(&self) -> Page {
        self.temp_page.map(|p| p.0).unwrap_or(self.base_page)
    }
}

fn note_page(note: Note) -> Option<Page> {
    match note {
        Note::C0 => Some(Page::Osc),
        Note::C1 => Some(Page::Vcf),
        Note::C2 => Some(Page::Vca),
        Note::C3 => Some(Page::Mod),
        _ => None
    }
}

const SHORT_PRESS: u32 = 250;

impl Service for Dw6000Control {
    fn start(&mut self, router: &mut Router) {
        let state: State = State {
            inner: Arc::new(spin::Mutex::new(InnerState {
                current_program: None,
                base_page: Page::Osc,
                temp_page: None,
            }))
        };

        // receive pads from beatstep
        // select active parameter page
        // or toggle on/off functions (on-board chorus, outboard arp, etc.)
        let page_state: State = state.clone();
        let page_if = router.create_virtual_interface(Handler::new(move |_now, event, _ctx, _spawn, _sched| {
            if let RouterEvent::Packet(packet) = event {
                if let Ok(msg) = Message::try_from(packet) {
                    let mut state = page_state.inner.lock();
                    match msg {
                        Message::NoteOn(_, note, _) => {
                            if let Some(page) = note_page(note) {
                                state.temp_page = Some((page, long_now()))
                            } else {
                                // TODO pad toggles
                            }
                        }

                        Message::NoteOff(_, note, _) => {
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
                        _ => {}
                    }
                }
            }
        }));

        let page_route = router.bind(
            Route::link(self.beatstep.interface, page_if)
        );
        self.routes.push(page_route);

        let dump_if = router.create_virtual_interface(Handler::new(move |_now, _event, ctx, _spawn, _sched| {
            if let Some(dump) = ctx.tags.get(&Tag::Dump(26)) {
                let mut state = state.inner.lock();
                state.current_program = Some((Program::from(dump.clone()), long_now()));
            }
        }));

        let dump_route_id = router.bind(
            Route::link(self.beatstep.interface, dump_if)
                .filter(capture_sysex(korg_dw6000::dump_response()))
        );
        self.routes.push(dump_route_id);
    }

    fn stop(&mut self, router: &mut Router) {
        todo!()
    }
}
