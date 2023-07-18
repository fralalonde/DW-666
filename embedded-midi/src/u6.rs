use crate::{Fill, Cull, MidiError};
use core::convert::TryFrom;
use core::result::Result;
use crate::u14::U14;

/// A primitive value that can be from 0-0x7F
#[derive(Copy, Clone, Debug, Eq, PartialOrd, PartialEq, Ord)]
pub struct U6(pub u8);

impl TryFrom<u8> for U6 {
    type Error = MidiError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > 0x7F {
            Err(MidiError::InvalidInteger)
        } else {
            Ok(U6(value))
        }
    }
}

/// Takes (LSB, MSB)
impl From<(U6, U6)> for U14 {
    fn from(pair: (U6, U6)) -> Self {
        let (lsb, msb) = pair;
        U14::try_from(((msb.0 as u16) << 7) | (lsb.0 as u16)).unwrap()
    }
}

impl From<U6> for u8 {
    fn from(value: U6) -> u8 {
        value.0
    }
}

impl Cull<u8> for U6 {
    fn cull(value: u8) -> U6 {
        const MASK: u8 = 0b0011_1111;
        let value = MASK & value;
        U6(value)
    }
}

impl Fill<u8> for U6 {
    fn fill(value: u8) -> U6 {
        match U6::try_from(value) {
            Ok(x) => x,
            _ => U6::MAX,
        }
    }
}

impl U6 {
    pub const MAX: U6 = U6(63);
    pub const MIN: U6 = U6(0);
}
