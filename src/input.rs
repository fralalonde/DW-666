use alloc::boxed::Box;
use embedded_hal::digital::v2::InputPin;
use rtic::cyccnt::{Duration, Instant};

const CYCLES_STEPPING: u64 = 1024 * 1024;

pub enum ScanEvent {
    ButtonDown,
    Encoder(i32),
}

pub trait Scan {
    fn scan(&mut self, now: u64) -> Option<ScanEvent>;
}

struct Observed<T> {
    state: T,
    time: u64,
}

impl<T> Observed<T> {
    fn init(now: u64, init: T) -> Self {
        Observed {
            state: init,
            time: now,
        }
    }
}

// dt, clk
type EncoderState = (bool, bool);

pub struct Encoder<DT, CLK> {
    dt_pin: DT,
    clk_pin: CLK,
    prev: Observed<EncoderState>,
}

pub fn encoder<DT, CLK>(now: u64, dt_pin: DT, clk_pin: CLK) -> Box<(dyn Scan + Sync + Send)>
where
    DT: 'static + InputPin + Sync + Send,
    CLK: 'static + InputPin + Sync + Send,
{
    Box::new(Encoder {
        prev: (Observed::init(
            now,
            (
                dt_pin.is_low().unwrap_or(false),
                clk_pin.is_low().unwrap_or(false),
            ),
        )),
        dt_pin,
        clk_pin,
    })
}

impl<DT: InputPin, CLK: InputPin> Scan for Encoder<DT, CLK> {
    fn scan(&mut self, now: u64) -> Option<ScanEvent> {
        let enc_code = (
            self.dt_pin.is_low().unwrap_or(false),
            self.clk_pin.is_low().unwrap_or(false),
        );
        if enc_code != self.prev.state {
            let elapsed: u64 = now - self.prev.time;
            // exponential stepping based on rotation speed
            let stonks = elapsed / CYCLES_STEPPING;
            let steps = match stonks {
                // TODO proportional stepping
                0 => return None, // too fast, debouncing
                1..=2 => 16,
                3..=4 => 8,
                5..=8 => 4,
                9..=16 => 2,
                _ => 1,
            };
            match (self.prev.state, enc_code) {
                ((false, true), (true, true)) => {
                    self.prev.time = now;
                    self.prev.state = enc_code;
                    return Some(ScanEvent::Encoder(steps));
                }
                ((true, false), (true, true)) => {
                    self.prev.time = now;
                    self.prev.state = enc_code;
                    return Some(ScanEvent::Encoder(-steps));
                }

                // TODO differential subcode speed stepping hint?
                _ => self.prev.state = enc_code,
            };
        }
        None
    }
}

// Button

struct ButtonState {
    pushed: bool,
}

pub struct Button<PIN> {
    btn_pin: PIN,
    prev: Observed<ButtonState>,
}

impl<T: InputPin> Scan for Button<T> {
    fn scan(&mut self, now: u64) -> Option<ScanEvent> {
        let pushed: bool = self.btn_pin.is_low().unwrap_or(false);
        if pushed != self.prev.state.pushed {
            self.prev.state.pushed = pushed;
            self.prev.time = now;
            // TODO button up, double click
            Some(ScanEvent::ButtonDown)
        } else {
            None
        }
    }
}
