use crate::midi::{Router, Service, Note, Endpoint, note_off, note_on, Velocity, channel};
use crate::{devices};
use alloc::vec::Vec;
use alloc::sync::Arc;
use crate::time::{TimeUnits, Tasks};


pub struct BlinkyBeat {
    state: Arc<spin::Mutex<InnerState>>,
}

#[derive(Debug)]
struct InnerState {
    beatstep: Endpoint,
    notes: Vec<(Note, bool)>,
}

impl InnerState {}

impl BlinkyBeat {
    pub fn new(beatstep: impl Into<Endpoint>, notes: Vec<Note>) -> Self {
        BlinkyBeat {
            state: Arc::new(spin::Mutex::new(InnerState {
                beatstep: beatstep.into(),
                notes: notes.into_iter().map(|n| (n, false)).collect(),
            })),
        }
    }
}

use devices::arturia::beatstep;
use beatstep::Param::*;
use beatstep::Pad::*;
use crate::devices::arturia::beatstep::{SwitchMode};

impl Service for BlinkyBeat {
    fn start(&mut self, now: rtic::cyccnt::Instant, _router: &mut Router, tasks: &mut Tasks) {
        let state = self.state.clone();
        tasks.enqueue(now, move |_now, _chaos, spawn| {
            let mut state = state.lock();
            let bs = state.beatstep;
            for sysex in devices::arturia::beatstep::beatstep_set(PadNote(Pad(15), channel(1), Note::C1m, SwitchMode::Gate)) {
                for packet in sysex {
                    spawn.send_midi(bs.interface, packet)?;
                }
            }
            for (note, ref mut on) in &mut state.notes {
                if *on {
                    spawn.send_midi(bs.interface, note_off(bs.channel, *note, Velocity::MAX).unwrap().into())?;
                } else {
                    spawn.send_midi(bs.interface, note_on(bs.channel, *note, Velocity::MIN).unwrap().into())?;
                }
                *on = !*on
            }
            Ok(Some(200.millis()))
        });

        rprintln!("BlinkyBeat Active")
    }
}
