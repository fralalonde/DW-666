use core::convert::TryFrom;
use crate::{MidiError, Cull, Fill};

/// A primitive value that can be from 0-0x7F
#[derive(Copy, Clone, Debug, Eq, PartialOrd, PartialEq, Ord)]
pub struct U4(pub u8);

impl TryFrom<u8> for U4 {
    type Error = MidiError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > U4::MAX.0 {
            Err(MidiError::InvalidInteger)
        } else {
            Ok(U4(value))
        }
    }
}

impl From<U4> for u8 {
    fn from(value: U4) -> u8 {
        value.0
    }
}

impl Cull<u8> for U4 {
    fn cull(value: u8) -> U4 {
        const MASK: u8 = 0x0F;
        U4(MASK & value)
    }
}

impl Fill<u8> for U4 {
    fn fill(value: u8) -> U4 {
        match U4::try_from(value) {
            Ok(x) => x,
            _ => U4::MAX,
        }
    }
}



impl U4 {
    pub const MAX: U4 = U4(0x0F);
    pub const MIN: U4 = U4(0);

    /// Returns (LSB, MSB)
    pub fn split(value: u8) -> (U4, U4) {
        (
            U4::cull((value & 0b1111) as u8),
            U4::cull((value >> 4) as u8)
        )
    }

}
