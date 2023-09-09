use runtime::{ExtU32, Local};

#[derive(Debug, Default)]
struct BounceApp {
    counter: u32,
}

impl BounceApp {}

static BOUNCE: Local<BounceApp> = Local::uninit("BOUNCE");

pub fn start_app() {
    BOUNCE.init_static(BounceApp { counter: 0 });

    runtime::spawn(async move {
        loop {
            // midisplay::spawn(format!("{}", state.counter)).unwrap();
            unsafe { BOUNCE.raw_mut() }.counter += 1;
            if runtime::delay(1000.millis()).await.is_err() {break}
        }
    });

    info!("Bounce Active");
}
