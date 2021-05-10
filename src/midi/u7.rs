use crate::midi::{Saturate, Cull, MidiError};
use core::convert::TryFrom;
use core::result::Result;
use crate::midi::u14::U14;

/// A primitive value that can be from 0-0x7F
#[derive(Copy, Clone, Debug, Eq, PartialOrd, PartialEq, Ord)]
pub struct U7(pub u8);

impl TryFrom<u8> for U7 {
    type Error = MidiError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > 0x7F {
            Err(MidiError::InvalidU7)
        } else {
            Ok(U7(value))
        }
    }
}

/// Takes (LSB, MSB)
impl From<(U7, U7)> for U14 {
    fn from(pair: (U7, U7)) -> Self {
        let (lsb, msb) = pair;
        U14::try_from(((msb.0 as u16) << 7) | (lsb.0 as u16)).unwrap()
    }
}

impl From<U7> for u8 {
    fn from(value: U7) -> u8 {
        value.0
    }
}

impl Cull<u8> for U7 {
    fn cull(value: u8) -> U7 {
        const MASK: u8 = 0b0111_1111;
        let value = MASK & value;
        U7(value)
    }
}

impl Saturate<u8> for U7 {
    fn saturate(value: u8) -> U7 {
        match U7::try_from(value) {
            Ok(x) => x,
            _ => U7::MAX,
        }
    }
}

impl U7 {
    pub const MAX: U7 = U7(0x7F);
    pub const MIN: U7 = U7(0);
}
