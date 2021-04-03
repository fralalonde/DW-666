use embedded_hal::digital::v2::InputPin;
use crate::event::{CtlEvent, RotaryId, RotaryEvent, ButtonId, Instant};
use enum_map::EnumMap;
use crate::event::RotaryEvent::{TickClockwise, TickCounterClockwise};
use crate::event::ButtonEvent::{Down, Up};

// const CYCLES_STEPPING: u64 = 1000;
//
// pub const SCAN_FREQ_HZ: u32 = 1_000;
// const SCAN_PERIOD_MICROS: u64 = ((1.0 / SCAN_FREQ_HZ as f64) * 1_000_000.0) as u64;

pub trait Scan {
    fn scan(&mut self, now: Instant) -> Option<CtlEvent>;
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
    fn scan(&mut self, now: Instant) -> Option<CtlEvent> {
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

            match (self.prev_state, new_state) {
                ((false, true), (true, true)) => {
                    self.prev_state = new_state;
                    return Some(CtlEvent::Rotary(self.source, TickClockwise(now)));
                }
                ((true, false), (true, true)) => {
                    self.prev_state = new_state;
                    return Some(CtlEvent::Rotary(self.source, TickCounterClockwise(now)));
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
    fn scan(&mut self, now: Instant) -> Option<CtlEvent> {
        let pushed: bool = self.btn_pin.is_low().unwrap_or(false);
        if pushed != self.prev_pushed {
            self.prev_pushed = pushed;
            // TODO button up, double click
            if pushed {
                Some(CtlEvent::Button(self.source, Down(now)))
            } else {
                Some(CtlEvent::Button(self.source, Up(now)))
            }
        } else {
            None
        }
    }
}

pub struct Controls<DT1, CLK1> {
    velocities: EnumMap<RotaryId, i8>,
    encoder1: Encoder<DT1, CLK1>,
}

impl <DT1: InputPin, CLK1: InputPin> Scan for Controls<DT1, CLK1> {
    fn scan(&mut self, now: u64) -> Option<CtlEvent> {
        self.encoder1.scan(now)
    }
}

impl <DT1, CLK1> Controls<DT1, CLK1> {

    pub fn new(encoder1: Encoder<DT1, CLK1>) -> Self {
        Controls {
            encoder1,
            velocities: EnumMap::new(),
        }
    }

    /// Emit derivatives events
    pub fn derive(&mut self, event: CtlEvent) -> Option<CtlEvent> {
        // let prev_time = self.velocities.
        match event {
            CtlEvent::Rotary(r, RotaryEvent::TickClockwise(_now)) => {
                Some(CtlEvent::Rotary(r, RotaryEvent::Turn(1)))
            }
            CtlEvent::Rotary(r, RotaryEvent::TickCounterClockwise(_now)) => {
                Some(CtlEvent::Rotary(r, RotaryEvent::Turn(-1)))
            }
            _ => None,
        }
    }
}