use midi::{ Note, Endpoint, note_off, note_on, Velocity, channel, MidiError, PacketList};
use crate::{devices, midi_route};
use alloc::vec::Vec;
use alloc::sync::Arc;

use devices::arturia::beatstep;
use beatstep::Param::*;
use beatstep::Pad::*;
use crate::devices::arturia::beatstep::{SwitchMode};
use crate::route::{Service};
use midi::Binding::Dst;

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
    fn start(&mut self) -> Result<(), MidiError> {
        let state = self.state.clone();
        runtime::spawn(async move {
           loop {
               let mut state = state.lock();
               let bs = state.beatstep;
               for sysex in devices::arturia::beatstep::beatstep_set(PadNote(Pad(0), channel(1), Note::C1m, SwitchMode::Gate)) {
                   midi_route(Dst(bs.interface), sysex.collect());
               }
               for (note, ref mut on) in &mut state.notes {
                   if *on {
                       midi_route(Dst(bs.interface), PacketList::single(note_on(bs.channel, *note, Velocity::MAX).unwrap().into()));
                   } else {
                       midi_route(Dst(bs.interface), PacketList::single(note_off(bs.channel, *note, Velocity::MIN).unwrap().into()));
                   }
                   *on = !*on
               }
               if runtime::delay_ms(2000).await.is_err() {break}
           }
        });

        info!("BlinkyBeat Active");
        Ok(())
    }
}
