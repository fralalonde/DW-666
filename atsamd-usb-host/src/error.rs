use crate::usb_host::TransferError;

impl From<PipeErr> for TransferError {
    fn from(v: PipeErr) -> Self {
        match v {
            PipeErr::TransferFail => Self::Retry("Transfer failed"),
            PipeErr::Flow => Self::Retry("Data flow"),
            PipeErr::DataToggle => Self::Retry("Toggle sequence"),

            PipeErr::ShortPacket => Self::Permanent("Pipe: Short packet"),
            PipeErr::InvalidPipe => Self::Permanent("Invalid pipe"),

            PipeErr::Stall => Self::Permanent("Pipe: Stall"),
            PipeErr::PipeErr => Self::Permanent("Pipe error"),
            PipeErr::HwTimeout => Self::Permanent("Pipe: Hardware timeout"),
            PipeErr::SwTimeout => Self::Permanent("Pipe: Software timeout"),
            PipeErr::NaksExceeded => Self::Permanent("Pipe: Naks Exceeded"),
            PipeErr::Other(s) => Self::Permanent(s),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, defmt::Format)]
#[allow(unused)]
pub(crate) enum PipeErr {
    ShortPacket,
    InvalidPipe,
    Stall,
    TransferFail,
    PipeErr,
    Flow,
    HwTimeout,
    DataToggle,
    SwTimeout,
    NaksExceeded,
    Other(&'static str),
}

impl From<&'static str> for PipeErr {
    fn from(v: &'static str) -> Self {
        Self::Other(v)
    }
}
