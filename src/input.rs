use rtic::cyccnt::{Instant, U32Ext as _, Duration};
use stm32f1xx_hal::gpio::gpioa::{PA5, PA6, PA7};
use stm32f1xx_hal::gpio::{Input, PullDown, PullUp};
use embedded_hal::digital::v2::InputPin;
use alloc::vec::Vec;
use alloc::boxed::Box;

const CYCLES_STEPPING: u32 = 2_000_000;

pub type Result<T: InputPin> = core::result::Result<T, T::Error>;

pub enum ScanEvent {
    ButtonDown,
    Encoder(i32),
}

pub trait Scan {
    fn scan(&mut self, now: Instant) -> Option<ScanEvent>;
}

struct Observed<T> {
    state: T,
    time: Instant,
}

impl <T> Observed<T> {
    fn init(now: Instant, init: T) -> Self {
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

pub fn encoder<DT, CLK>(now: Instant, dt_pin: DT, clk_pin: CLK) -> Box<(dyn Scan + Sync + Send)>
    where DT: 'static + InputPin + Sync + Send, CLK: 'static +  InputPin + Sync + Send
{
    Box::new(Encoder{
        prev: (Observed::init(now, (dt_pin.is_low().unwrap_or(false), clk_pin.is_low().unwrap_or(false)))),
        dt_pin,
        clk_pin,
    })
}

impl <DT: InputPin, CLK: InputPin> Scan for Encoder<DT, CLK> {
    fn scan(&mut self, now: Instant) -> Option<ScanEvent> {
        let enc_code = (self.dt_pin.is_low().unwrap_or(false), self.clk_pin.is_low().unwrap_or(false));
        if enc_code != self.prev.state {
            let elapsed: Duration = now - self.prev.time;
            // exponential stepping based on rotation speed
            let steps = match elapsed.as_cycles() / CYCLES_STEPPING {
                // TODO proportional stepping
                // 0 => 16,
                0..=1 => 16,
                2..=4 => 4,
                // 4..=6 => 2,
                _ => 1,
            };
            match (self.prev.state, enc_code) {
                ((false, true), (true, true)) => {
                    self.prev.time = now;
                    return Some(ScanEvent::Encoder(steps))
                }
                ((true, false), (true, true)) => {
                    self.prev.time = now;
                    return Some(ScanEvent::Encoder(-steps))
                }
                // TODO differential subcode speed stepping hint
                _ => {}
            };
            self.prev.state = enc_code;
        }
        None
    }
}

// Button

struct ButtonState {
    pushed: bool,
}

pub struct Button<BTN_PIN> {
    btn_pin: BTN_PIN,
    prev: Observed<ButtonState>,
}

impl <T: InputPin> Scan for Button<T> {
    fn scan(&mut self, now: Instant) -> Option<ScanEvent> {
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