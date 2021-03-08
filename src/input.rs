use embedded_hal::digital::v2::InputPin;

const CYCLES_STEPPING: u64 = 1000;

pub const SCAN_FREQ_HZ: u32 = 1_000;
const SCAN_PERIOD_MICROS: u64 = ((1.0 / SCAN_FREQ_HZ as f64) * 1_000_000.0) as u64;

#[derive(Copy, Clone)]
pub enum Source {
    Encoder1,
}

pub enum Event {
    ButtonDown(Source),
    ButtonUp(Source),
    EncoderTurn(Source, i32),
}

pub trait Scan {
    fn scan(&mut self) -> Option<Event>;
}

struct Observed<T> {
    state: T,
    time: u64,
}

impl<T> Observed<T> {
    fn init(init: T) -> Self {
        Observed {
            state: init,
            time: 0,
        }
    }
}

// dt, clk
type EncoderState = (bool, bool);

pub struct Encoder<DT, CLK> {
    source: Source,
    dt_pin: DT,
    clk_pin: CLK,
    prev: Observed<EncoderState>,
}

pub fn encoder<DT, CLK>(source: Source, dt_pin: DT, clk_pin: CLK) -> Encoder<DT, CLK>
where
    DT: 'static + InputPin + Sync + Send,
    CLK: 'static + InputPin + Sync + Send,
{
    Encoder {
        source,
        prev: (Observed::init(
            (
                dt_pin.is_low().unwrap_or(false),
                clk_pin.is_low().unwrap_or(false),
            ),
        )),
        dt_pin,
        clk_pin,
    }
}

impl<DT: InputPin, CLK: InputPin> Scan for Encoder<DT, CLK> {
    fn scan(&mut self) -> Option<Event> {
        let enc_code = (
            self.dt_pin.is_low().unwrap_or(false),
            self.clk_pin.is_low().unwrap_or(false),
        );
        if enc_code != self.prev.state {
            // exponential stepping based on rotation speed
            let stonks = SCAN_PERIOD_MICROS / CYCLES_STEPPING;
            let steps = match stonks {
                // TODO proportional stepping
                0 => return None, // too fast (debouncing)
                1..=2 => 16,
                3..=4 => 8,
                5..=8 => 4,
                9..=16 => 2,
                _ => 1,
            };
            match (self.prev.state, enc_code) {
                ((false, true), (true, true)) => {
                    self.prev.state = enc_code;
                    return Some(Event::EncoderTurn(self.source, steps));
                }
                ((true, false), (true, true)) => {
                    self.prev.state = enc_code;
                    return Some(Event::EncoderTurn(self.source, -steps));
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
    source: Source,
    btn_pin: PIN,
    prev: Observed<ButtonState>,
}

impl<T: InputPin> Scan for Button<T> {
    fn scan(&mut self) -> Option<Event> {
        let pushed: bool = self.btn_pin.is_low().unwrap_or(false);
        if pushed != self.prev.state.pushed {
            self.prev.state.pushed = pushed;
            // TODO button up, double click
            if pushed {
                Some(Event::ButtonDown(self.source))
            } else {
                Some(Event::ButtonUp(self.source))
            }
        } else {
            None
        }
    }
}
