use midi::{ Note, Endpoint, note_off, note_on, Velocity, channel, MidiError, PacketList};
use crate::{devices, app};
use alloc::vec::Vec;
use alloc::sync::Arc;
use crate::time::{/*TimeUnits,*/ Tasks};
use devices::arturia::beatstep;
use beatstep::Param::*;
use beatstep::Pad::*;
use crate::devices::arturia::beatstep::{SwitchMode};
use crate::route::{Router, Service};
use midi::Binding::Dst;
use rtic::rtic_monotonic::Milliseconds;


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


impl Service for BlinkyBeat {
    fn start(&mut self, _router: &mut Router, tasks: &mut Tasks) -> Result<(), MidiError> {
        let state = self.state.clone();
        tasks.repeat(move |_chaos| {
            let mut state = state.lock();
            let bs = state.beatstep;
            for sysex in devices::arturia::beatstep::beatstep_set(PadNote(Pad(0), channel(1), Note::C1m, SwitchMode::Gate)) {
                app::midispatch::spawn(Dst(bs.interface), sysex.collect()).unwrap();
            }
            for (note, ref mut on) in &mut state.notes {
                if *on {
                    app::midispatch::spawn(Dst(bs.interface), PacketList::single(note_on(bs.channel, *note, Velocity::MAX)?.into()))?;
                } else {
                    app::midispatch::spawn(Dst(bs.interface), PacketList::single(note_off(bs.channel, *note, Velocity::MIN)?.into()))?;
                }
                *on = !*on
            }
            Ok(Some(Milliseconds(2000)))
        });

        rprintln!("BlinkyBeat Active");
        Ok(())
    }
}
