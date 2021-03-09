use crate::{ event};

use crate::midi::u4::U4;
use crate::midi::notes::Note;
use core::convert::TryFrom;

use num_enum::TryFromPrimitive;
use crate::event::{UiEvent,  RotaryEvent, AppEvent, Param};



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

#[derive(Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MenuDisplay {
    Channel,
    Note,
}

/// Local appearance, transient, not directly sound related
#[derive(Clone)]
pub struct MenuState {
    pub selected: MenuDisplay,
}

impl Default for ArpState {
    fn default() -> Self {
        ArpState {
            channel: U4::MIN,
            note: Note::C4,
        }
    }
}

impl ArpState {
    pub fn bump(&mut self) {
        self.note = Note::try_from(self.note as u8  + 1).unwrap_or(Note::C4);
        self.channel = U4::try_from(u8::from(self.channel) + 1).unwrap_or(U4::MIN)
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
    pub fn dispatch(&mut self, event: event::UiEvent) -> Option<AppEvent> {
        match event {
            UiEvent::Rotary(_r, RotaryEvent::Turn(delta)) => {
                self.patch.filter_cutoff += delta;
                Some(AppEvent::ParamChange(Param::FilterCutoff(self.patch.filter_cutoff)))
            }
            UiEvent::Button(_, _) => {
                // TODO select next menu item
                None
            }
            _ => None,
        }
    }
}
