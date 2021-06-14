use midi::{ Note, Endpoint, note_off, note_on, Velocity, channel, MidiError};
use crate::{devices, route};
use alloc::vec::Vec;
use alloc::sync::Arc;
use crate::time::{TimeUnits, Tasks};


pub struct Bounce {
    state: Arc<spin::Mutex<InnerState>>,
}

#[derive(Debug)]
struct InnerState {
    counter: u32,
}

impl InnerState {}

impl Bounce {
    pub fn new() -> Self {
        Bounce {
            state: Arc::new(spin::Mutex::new(InnerState {
                counter: 0
            })),
        }
    }
}

use devices::arturia::beatstep;
use beatstep::Param::*;
use beatstep::Pad::*;
use crate::devices::arturia::beatstep::{SwitchMode};
use crate::Binding::Dst;

impl route::Service for Bounce {
    fn start(&mut self, now: rtic::cyccnt::Instant, _router: &mut route::Router, tasks: &mut Tasks) -> Result<(), MidiError> {
        let state = self.state.clone();
        tasks.repeat(now, move |_now, _chaos, spawn| {
            let mut state = state.lock();
            spawn.midisplay(format!("{}", state.counter)).unwrap();
            state.counter += 1;
            Ok(Some(1000.millis()))
        });

        rprintln!("Bounce Active");
        Ok(())
    }
}
