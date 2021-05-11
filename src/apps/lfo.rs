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
    Sine,
    Saw,
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

impl Lfo {
    pub fn mod_value(&mut self, froot: f32, time: BigInstant, chaos: &mut WyRand) -> f32 {
        match self.wave {
            Waveform::Sine => {
                let timex = time.0 as f32 / self.period;
                let modulation = timex.sin() * self.amount;
                let modulated = froot + modulation;
                // rprintln!("mod ftimex {:.3} timex {:.3} xsin {:.3} amt {:.3} mod {:.3} root {:.3} mdd {:.3}", ftime, timex, timex.sin(),  self.amount, modulation, froot, modulated);

                modulated.max(0.0).min(1.0)
            }
            Waveform::Square => {
                // if let Some(mut ftime) = f32::from_u64(time.0 >> 20) {
                //     let x = ftime * self.rate_hz;
                //     let modulation = x.sin() * self.amount;
                //     let modulated = froot + (froot * modulation);
                //     rprintln!("mod amt {} mod {} root {} mdd {}", self.amount, modulation, froot, modulated);
                //
                //     unsafe { modulated.to_int_unchecked() }
                // } else {
                //     root_value
                // }
                0.0
            }
            Waveform::Saw => {
                let timex = time.0 as f32 / self.period;
                let modulation = timex.fract() * self.amount;
                let modulated = froot + modulation;
                // rprintln!("mod ftimex {:.3} timex {:.3} xsin {:.3} amt {:.3} mod {:.3} root {:.3} mdd {:.3}", ftime, timex, timex.sin(),  self.amount, modulation, froot, modulated);

                modulated.max(0.0).min(1.0)
            }
            Waveform::Random => chaos.generate_range::<u32>(0, u32::MAX) as f32,
        }
    }

    // pub fn next_iter(&self) -> Duration {
    //     (250 * CYCLES_PER_MILLISEC).cycles()
    // }

    pub fn get_amount(&self) -> f32 {
        self.amount
    }

    pub fn set_amount(&mut self, mut amount: f32) {
        amount = amount.max(0.0).min(1.0);
        self.amount = amount
    }

    pub fn get_rate_hz(&self) -> f32 {
        1.0 / (self.period / F_CPU_FREQ)
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