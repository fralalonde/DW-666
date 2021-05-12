use core::sync::atomic::{AtomicI32, AtomicU32};
use nanorand::{WyRand, RNG};
use rtic::cyccnt::{Duration, U32Ext};
use crate::{CYCLES_PER_MILLISEC, CPU_FREQ};
use crate::clock::BigInstant;
use num_enum::{FromPrimitive};
use num::FromPrimitive as _;
use core::f32;
use micromath::F32Ext;

#[derive(Debug, FromPrimitive, Copy, Clone)]
#[repr(u8)]
pub enum Waveform {
    #[num_enum(default)]
    Triangle,
    Sine,
    Saw,
    RevSaw,
    Square,
    Random,
}

impl Default for Waveform {
    fn default() -> Self {
        Waveform::Sine
    }
}

#[derive(Debug, Default)]
pub struct Lfo {
    period: f32,
    // between 0 and 1
    amount: f32,
    wave: Waveform,
}

const F_CPU_FREQ: f32 = CPU_FREQ as f32;

// I am quite aware that these computations are possible HORRIBLY INNEFICIENT and naive. IJDGAF.
impl Lfo {
    pub fn mod_value(&mut self, froot: f32, time: BigInstant, chaos: &mut WyRand) -> f32 {
        (froot + match self.wave {
            Waveform::Triangle => {
                let timex = time.0 as f32 % self.period;
                let half = self.period / 2.0;
                let mut modulation = timex / half;
                if timex > half {
                    modulation = 1.0 - modulation;
                }
                (modulation - 0.5) * 2.0 * self.amount
            }
            Waveform::Sine => {
                let timex = time.0 as f32 / self.period;
                timex.sin() * self.amount
            }
            Waveform::Square => {
                let timex = time.0 as f32 % self.period;
                let half = self.period / 2.0;
                (if timex > half { 1.0 } else { -1.0 }) * self.amount
            }
            Waveform::Saw => {
                let timex = time.0 as f32 / self.period;
                ((1.0 - timex.fract()) - 0.5) * 2.0 * self.amount
            }
            Waveform::RevSaw => {
                let timex = time.0 as f32 / self.period;
                (timex.fract() - 0.5) * 2.0 * self.amount
            }
            Waveform::Random => ((chaos.generate_range::<u32>(0, u32::MAX) as f32 / u32::MAX as f32) - 0.5) * 2.0 * self.amount
        }).max(0.0).min(1.0)
    }

    pub fn get_amount(&self) -> f32 {
        self.amount
    }

    pub fn set_amount(&mut self, mut amount: f32) {
        amount = amount.max(0.0).min(1.0);
        self.amount = amount
    }

    pub fn get_rate_hz(&self) -> f32 {
        F_CPU_FREQ / self.period
    }

    pub fn set_rate_hz(&mut self, mut rate: f32) {
        self.period = F_CPU_FREQ / rate;
    }

    pub fn get_waveform(&self) -> Waveform {
        self.wave
    }

    pub fn set_waveform(&mut self, wave: Waveform) {
        self.wave = wave;
    }
}