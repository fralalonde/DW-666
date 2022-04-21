use midi::{MidiError};
use crate::{route};
use alloc::sync::Arc;

pub struct Bounce {
    state: Arc<spin::Mutex<InnerState>>,
}

#[derive(Debug)]
struct InnerState {
    counter: u32,
}

impl InnerState {}

impl route::Service for Bounce {
    fn start(&mut self) -> Result<(), MidiError> {
        let state = self.state.clone();
        runtime::spawn(async move {
            loop {
                let mut state = state.lock();
                // midisplay::spawn(format!("{}", state.counter)).unwrap();
                state.counter += 1;
                if runtime::delay_ms(1000).await.is_err() {break}
            }
        });

        info!("Bounce Active");
        Ok(())
    }
}
