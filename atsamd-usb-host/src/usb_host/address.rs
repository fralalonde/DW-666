use hash32::Hasher;

const MAX_DEVICES: u8 = 127;

#[derive(Clone, Copy, Debug, defmt::Format, Eq, PartialEq)]
pub struct Address(u8);

impl hash32::Hash for Address {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        state.write(&[self.0])
    }
}

pub struct AddressPool {
    pool: u128,
}

impl From<u8> for Address {
    fn from(addr: u8) -> Self {
        if addr > MAX_DEVICES { panic!("USB addr out of range") }
        Address(addr)
    }
}

impl From<Address> for u8 {
    fn from(addr: Address) -> Self {
        addr.0
    }
}

impl AddressPool {
    pub(crate) fn new() -> Self {
        Self {
            pool: u128::MAX - 1,
        }
    }

    pub(crate) fn take_next(&mut self) -> Option<Address> {
        let next = self.pool.leading_zeros() as u8;
        if next <= MAX_DEVICES {
            self.pool &= !(1 << next);
            return Some(Address::from(next));
        }
        None
    }

    pub(crate) fn put_back(&mut self, addr: Address) {
        let addr: u8 = addr.into();
        if addr <= MAX_DEVICES {
            self.pool |= 1 << addr
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn take_and_put_back_one() {

    }

    #[test]
    fn take_and_put_back_all() {

    }

    #[test]
    fn take_too_many() {

    }
}