use crate::{Fill, Cull, MidiError};
use core::convert::TryFrom;
use core::result::Result;
use crate::u14::U14;

/// A primitive value that can be from 0-0x7F
#[derive(Copy, Clone, Debug, Eq, PartialOrd, PartialEq, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct U7(pub u8);

impl TryFrom<u8> for U7 {
    type Error = MidiError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > 0x7F {
            Err(MidiError::InvalidInteger)
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

impl Fill<u8> for U7 {
    fn fill(value: u8) -> U7 {
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
