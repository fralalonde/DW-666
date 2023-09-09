use midi::{Note,  note_off, note_on, Velocity, PacketList, MidiInterface, MidiChannel};
use crate::{devices, midi_send};
use alloc::vec::Vec;

use devices::arturia::beatstep;
use beatstep::Param::*;
use beatstep::Pad::*;
use crate::devices::arturia::beatstep::{SwitchMode};

use runtime::{Local};

use runtime::ExtU32;

#[derive(Debug)]
struct InnerState {
    channel: MidiChannel,
    notes: Vec<(Note, bool)>,
}

impl InnerState {}

static BLINKY_BEAT: Local<InnerState> = Local::uninit("BLINKY_BEAT");

/// MIDI Interface to BeatStep through MIDI USB Coprocessor
const IF_BEATSTEP: MidiInterface = MidiInterface::Serial(1);

pub fn start_app(channel: MidiChannel, notes: &[Note]) {
    BLINKY_BEAT.init_static(InnerState {
        channel,
        notes: notes.iter().map(|n| (*n, false)).collect(),
    });
    runtime::spawn(async move {
        loop {
            let z = unsafe { BLINKY_BEAT.raw_mut() };
            for sysex in devices::arturia::beatstep::beatstep_set(PadNote(Pad(0), z.channel, Note::C1m, SwitchMode::Gate)) {
                midi_send(IF_BEATSTEP, sysex.collect());
            }
            for (note, ref mut on) in &mut z.notes {
                if *on {
                    midi_send(IF_BEATSTEP, PacketList::single(note_on(MidiChannel(0), *note, Velocity::MAX).unwrap().into()));
                } else {
                    midi_send(IF_BEATSTEP, PacketList::single(note_off(MidiChannel(0), *note, Velocity::MIN).unwrap().into()));
                }
                *on = !*on
            }
            if runtime::delay(2000.millis()).await.is_err() { break; }
        }
    });

    info!("BlinkyBeat Active");
}
