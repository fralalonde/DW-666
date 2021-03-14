use embedded_hal::digital::v2::InputPin;
use crate::event::{UiEvent, RotaryId, RotaryEvent, ButtonId, Instant};
use enum_map::EnumMap;
use crate::event::RotaryEvent::{TickClockwise, TickCounterClockwise};
use crate::event::ButtonEvent::{Down, Up};

use rtt_target::{rprintln, rtt_init_print};

const CYCLES_STEPPING: u64 = 1000;

pub const SCAN_FREQ_HZ: u32 = 1_000;
const SCAN_PERIOD_MICROS: u64 = ((1.0 / SCAN_FREQ_HZ as f64) * 1_000_000.0) as u64;

pub trait Scan {
    fn scan(&mut self, now: Instant) -> Option<UiEvent>;
}

// dt, clk
type EncoderState = (bool, bool);

pub struct Encoder<DT, CLK> {
    source: RotaryId,
    dt_pin: DT,
    clk_pin: CLK,
    prev_state: EncoderState,
}

pub fn encoder<DT, CLK>(source: RotaryId, dt_pin: DT, clk_pin: CLK) -> Encoder<DT, CLK>
    where
        DT: 'static + InputPin + Sync + Send,
        CLK: 'static + InputPin + Sync + Send,
{
    Encoder {
        source,
        prev_state: (
            dt_pin.is_low().unwrap_or(false),
            clk_pin.is_low().unwrap_or(false),
        ),
        dt_pin,
        clk_pin,
    }
}

impl<DT: InputPin, CLK: InputPin> Scan for Encoder<DT, CLK> {
    fn scan(&mut self, now: Instant) -> Option<UiEvent> {
        let new_state = (
            self.dt_pin.is_low().unwrap_or(false),
            self.clk_pin.is_low().unwrap_or(false),
        );

        if new_state != self.prev_state {
            // exponential stepping based on rotation speed
            // let stonks = SCAN_PERIOD_MICROS / CYCLES_STEPPING;
            // let steps = match stonks {
            //     // TODO proportional stepping
            //     0 => return None, // too fast (debouncing)
            //     1..=2 => 16,
            //     3..=4 => 8,
            //     5..=8 => 4,
            //     9..=16 => 2,
            //     _ => 1,
            // };
            rprintln!("PQOIQP");

            match (self.prev_state, new_state) {
                ((false, true), (true, true)) => {
                    rprintln!("GEUEGEU");
                    self.prev_state = new_state;
                    return Some(UiEvent::Rotary(self.source, TickClockwise(now)));
                }
                ((true, false), (true, true)) => {
                    rprintln!("AGAGA");
                    self.prev_state = new_state;
                    return Some(UiEvent::Rotary(self.source, TickCounterClockwise(now)));
                }

                // TODO differential subcode speed stepping hint?
                _ => self.prev_state = new_state,
            };
        }
        None
    }
}

pub struct Button<PIN> {
    source: ButtonId,
    btn_pin: PIN,
    prev_pushed: bool,
}

impl<T: InputPin> Scan for Button<T> {
    fn scan(&mut self, now: Instant) -> Option<UiEvent> {
        let pushed: bool = self.btn_pin.is_low().unwrap_or(false);
        if pushed != self.prev_pushed {
            self.prev_pushed = pushed;
            // TODO button up, double click
            if pushed {
                Some(UiEvent::Button(self.source, Down(now)))
            } else {
                Some(UiEvent::Button(self.source, Up(now)))
            }
        } else {
            None
        }
    }
}

pub struct Controls {
    velocities: EnumMap<RotaryId, i8>,
}

impl Controls {
    pub fn dispatch(&mut self, event: UiEvent) -> Option<UiEvent> {
        match event {
            UiEvent::Rotary(r, RotaryEvent::TickClockwise(_now)) => {
                Some(UiEvent::Rotary(r, RotaryEvent::Turn(1)))
            }
            UiEvent::Rotary(r, RotaryEvent::TickCounterClockwise(_now)) => {
                Some(UiEvent::Rotary(r, RotaryEvent::Turn(-1)))
            }
            _ => None,
        }
    }
}