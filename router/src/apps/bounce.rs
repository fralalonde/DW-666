use midi::{MidiError};
use crate::{route};
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
