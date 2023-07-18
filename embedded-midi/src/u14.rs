use crate::{Fill, Cull, MidiError};
use core::convert::TryFrom;
use crate::u7::U7;

/// A primitive value that can be from 0-0x7F
#[derive(Copy, Clone, Debug, Eq, PartialOrd, PartialEq, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct U14(pub u16);

impl TryFrom<u16> for U14 {
    type Error = MidiError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 0b11_1111_1111_1111 {
            Err(MidiError::InvalidInteger)
        } else {
            Ok(U14(value))
        }
    }
}

impl TryFrom<(u8, u8)> for U14 {
    type Error = MidiError;

    fn try_from(value: (u8, u8)) -> Result<Self, Self::Error> {
        let (lsb, msb) = value;
        Ok(U14::from((U7::try_from(lsb)?, U7::try_from(msb)?)))
    }
}

/// Returns (LSB, MSB)
impl From<U14> for (U7, U7) {
    fn from(value: U14) -> Self {
        (
            U7::fill((value.0 & 0b1111111) as u8),
            U7::fill((value.0 >> 7) as u8)
        )
    }
}

impl From<U14> for u16 {
    fn from(value: U14) -> u16 {
        value.0
    }
}

impl Cull<u16> for U14 {
    fn cull(value: u16) -> U14 {
        const MASK: u16 = 0b11_1111_1111_1111;
        let value = MASK & value;
        U14(value)
    }
}

impl Fill<u16> for U14 {
    fn fill(value: u16) -> U14 {
        match U14::try_from(value) {
            Ok(x) => x,
            _ => U14::MAX,
        }
    }
}

impl U14 {
    pub const MAX: U14 = U14(0b11_1111_1111_1111);
    pub const MIN: U14 = U14(0);
}
