#[derive(Clone, Copy, Debug)]
#[allow(unused)]
pub enum Sysex {
    Begin(u8, u8),
    Cont(u8, u8, u8),
    End,
    End1(u8),
    End2(u8, u8),
}
