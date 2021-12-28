use midi::{MidiError};
use crate::{route};
use alloc::sync::Arc;
use embedded_time::duration::Milliseconds;
use crate::time::{Tasks};

pub struct Bounce {
    state: Arc<spin::Mutex<InnerState>>,
}

#[derive(Debug)]
struct InnerState {
    counter: u32,
}

impl InnerState {}

impl route::Service for Bounce {
    fn start(&mut self, _router: &mut route::Router, tasks: &mut Tasks) -> Result<(), MidiError> {
        let state = self.state.clone();
        tasks.repeat(move |_chaos| {
            let mut state = state.lock();
            crate::app::midisplay::spawn(format!("{}", state.counter)).unwrap();
            state.counter += 1;
            Ok(Some(Milliseconds(1000)))
        });

        rprintln!("Bounce Active");
        Ok(())
    }
}
