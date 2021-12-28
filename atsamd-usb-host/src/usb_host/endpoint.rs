use crate::Address;
use crate::usb_host::TransferType;

#[derive(Copy, Clone, Debug)]
pub enum Toggle {
    Data0,
    Data1
}

impl Toggle {
    fn flip(self) -> Self {
        match self {
            Toggle::Data0 => Toggle::Data1,
            Toggle::Data1 => Toggle::Data0
        }
    }
}

/// USB endpoint parameters and state
#[derive(Copy, Clone, Debug)]
#[repr(packed)]
pub struct Endpoint {
    /// Address of the device owning this endpoint
    pub(crate) device_address: Address,

    pub(crate) endpoint_address: u8,

    /// The maximum packet size for this endpoint
    pub(crate) max_packet_size: u16,

    pub(crate) transfer_type: TransferType,

    // TODO merge to single byte, XOR single bits
    pub(crate) in_toggle: Toggle,
    pub(crate) out_toggle: Toggle,
}

impl Endpoint {
    pub(crate) fn flip_toggle_in(&mut self) -> Toggle {
        self.in_toggle = self.in_toggle.flip();
        self.in_toggle
    }

    pub(crate) fn flip_toggle_out(&mut self) -> Toggle {
        self.out_toggle = self.out_toggle.flip();
        self.out_toggle
    }
}

impl Endpoint {
    pub fn with_endpoint_addr(&self, ep_addr: u8, transfer_type: TransferType) -> Endpoint {
        let mut new = self.clone();
        new.endpoint_address = ep_addr;
        new.transfer_type = transfer_type;
        new
    }
}

