use core::sync::atomic::{AtomicI32, AtomicU32};
use nanorand::{WyRand, RNG};
use rtic::cyccnt::{Duration, U32Ext};
use crate::CYCLES_PER_MILLISEC;
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
    Square,
    Saw,
    Random,
}

impl Default for Waveform {
    fn default() -> Self {
        Waveform::Sine
    }
}

#[derive(Debug, Default)]
pub struct Lfo {
    rate_hz: f32,
    // between 0 and 1
    amount: f32,
    wave: Waveform,
}

impl Lfo {
    pub fn mod_value(&mut self, root_value: u8, time: BigInstant, chaos: &mut WyRand) -> u8 {
        match self.wave {
            Waveform::Sine => {
                if let Some(mut ftime) = f32::from_u64(time.0 >> 20) {
                    let x = ftime % self.rate_hz;
                    let modulation = (x.sin()) * self.amount;
                    let froot: f32 = f32::from(root_value);
                    let modulated = froot + (froot * modulation);

                    unsafe {modulated.to_int_unchecked() }
                } else {
                    root_value
                }
            },
            Waveform::Square => {
                if let Some(mut ftime) = f32::from_u64(time.0 >> 20) {
                    let x = ftime % self.rate_hz;
                    let modulation = (x.sin()) * self.amount;
                    let froot: f32 = f32::from(root_value);
                    let modulated = froot + (froot * 0.5);

                    unsafe {modulated.to_int_unchecked() }
                } else {
                    root_value
                }
            }
            Waveform::Saw => {
                0u8
            }
            Waveform::Random => (chaos.generate_range::<u32>(u32::MIN, u32::MAX) >> 24) as u8
        }
    }

    pub fn next_iter(&self) -> Duration {
        (250 * CYCLES_PER_MILLISEC).cycles()
    }
    pub fn get_amount(&self) -> f32 {
        self.amount
    }

    pub fn set_amount(&mut self, mut amount: f32) {
        amount = amount.max(1.0).min(0.0);
        self.amount = amount
    }

    pub fn get_rate_hz(&self) -> f32 {
        self.rate_hz
    }

    pub fn set_rate_hz(&mut self, mut rate: f32) {
        rate = rate.min(0.03).max(40.0);
        self.rate_hz = rate
    }

    pub fn get_waveform(&self) -> Waveform {
        self.wave
    }

    pub fn set_waveform(&mut self, wave: Waveform) {
        self.wave = wave;
    }
}