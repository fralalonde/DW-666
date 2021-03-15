use crate::input;

use crate::midi::packet::MidiPacket;
use crate::midi::u4::U4;
use crate::midi::notes::Note;
use core::convert::TryFrom;
use crate::event::{AppEvent, CtlEvent, RotaryEvent};
use crate::event::Param::FilterCutoff;
use crate::event::Config::MidiEcho;

/// Globals
#[derive(Clone, Default)]
pub struct ConfigState {
    echo_midi: bool,
}

/// Local appearance, transient, not directly sound related
#[derive(Clone)]
pub struct ArpState {
    pub channel: U4,
    pub note: Note,
}

impl Default for ArpState {
    fn default() -> Self {
        ArpState {
            channel: U4::try_from(09).unwrap(),
            note: Note::C4,
        }
    }
}

impl ArpState {
    pub fn bump(&mut self) {
        self.note = Note::try_from(self.note as u8  + 1).unwrap_or(Note::C4);
        // self.channel = U4::try_from(u8::from(self.channel) + 1).unwrap_or(U4::MIN)
    }
}

/// Local appearance, transient, not directly sound related
#[derive(Clone, Default)]
pub struct UiState {
    pub led_on: bool,
    pub last_error: &'static str,
}

/// Sound parameters
#[derive(Clone, Default)]
pub struct PatchState {
    filter_cutoff: i32,
}

#[derive(Clone, Default)]
/// The application state
pub struct AppState {
    pub config: ConfigState,
    pub patch: PatchState,
    pub ui: UiState,
    pub arp: ArpState,
}

impl AppState {
    pub fn set_echo_midi(&mut self, echo: bool) {
        self.config.echo_midi = echo
    }
}

impl AppState {
    pub fn dispatch_ctl(&mut self, event: CtlEvent) -> Option<AppEvent> {
        match event {
            CtlEvent::Rotary(_r, RotaryEvent::Turn(delta)) => {
                self.patch.filter_cutoff += delta;
                Some(AppEvent::ParamChange(FilterCutoff(self.patch.filter_cutoff)))
            }
            CtlEvent::Button(_, _) => {
                // TODO select next menu item
                None
            }
            _ => None,
        }
    }
}

impl AppState {
    pub fn midi_update(&mut self, _packet: MidiPacket) -> Option<AppEvent> {
        None
    }

}
