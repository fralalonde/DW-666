use core::sync::atomic::{AtomicI32, AtomicU32};
use nanorand::{WyRand, RNG};
use rtic::cyccnt::{Duration, U32Ext};
use crate::CYCLES_PER_MILLISEC;
use crate::clock::BigInstant;

#[derive(Debug)]
pub enum Waveform {
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
    rate: AtomicU32,
    rate_scale: AtomicI32,
    amplitude: AtomicU32,
    offset: AtomicU32,
    waveform: Waveform,
}

impl Lfo {
    pub fn update_value(&mut self, time: BigInstant, chaos: &mut WyRand) -> u32 {
        chaos.generate_range(u32::MIN, u32::MAX)
    }

    pub fn next_iter(&self) -> Duration {
        (250 * CYCLES_PER_MILLISEC).cycles()
    }
}