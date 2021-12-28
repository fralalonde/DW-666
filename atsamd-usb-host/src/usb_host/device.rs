use core::cmp::min;

use crate::DeviceDescriptor;
use crate::usb_host::{ConfigurationDescriptor, DescriptorType, EndpointDescriptor, RequestCode, RequestDirection, RequestKind, RequestRecipient, RequestType, to_slice_mut, Toggle, TransferError, TransferType, USBHost, WValue};
use crate::usb_host::address::{Address};
use crate::usb_host::endpoint::Endpoint;

#[derive(Copy, Clone, Debug, PartialEq)]
enum DeviceState {
    Init,
    Settling(u64),
    GetConfig,
    SetConfig(u8),
    SetProtocol,
    SetIdle,
    SetReport,
    Running,
}

#[derive(Debug)]
pub struct Device {
    control_ep: Endpoint,
    state: DeviceState,
}

impl Device {
    pub fn new(max_bus_packet_size: u16) -> Self {
        Self {
            state: DeviceState::Init,
            control_ep: Endpoint {
                device_address: Address::from(0),
                endpoint_address: 0,
                transfer_type: TransferType::Control,
                max_packet_size: max_bus_packet_size,
                in_toggle: Toggle::Data0,
                out_toggle: Toggle::Data1
            },
        }
    }

    pub fn endpoint(&self, desc: &EndpointDescriptor) -> Endpoint {
        let mut new = self.control_ep.clone();
        new.endpoint_address = desc.b_endpoint_address;
        new.transfer_type = TransferType::from(desc.bm_attributes);
        new.max_packet_size = desc.w_max_packet_size;
        new
    }

    /// Generic USB read control transfer
    pub fn control_get(&mut self, host: &mut dyn USBHost, desc_type: DescriptorType, idx: u8, buffer: &mut [u8]) -> Result<usize, TransferError> {
        host.control_transfer(
            &mut self.control_ep,
            RequestType::from((RequestDirection::DeviceToHost, RequestKind::Standard, RequestRecipient::Device)),
            RequestCode::GetDescriptor,
            WValue::from((idx, desc_type as u8)),
            0,
            Some(buffer),
        )
    }

    /// Generic USB write control transfer
    pub fn control_set(&mut self, host: &mut dyn USBHost, param: RequestCode, value: u8) -> Result<(), TransferError> {
        host.control_transfer(
            &mut self.control_ep,
            RequestType::from((RequestDirection::HostToDevice, RequestKind::Standard, RequestRecipient::Device)),
            param,
            WValue::from((value, 0)),
            0,
            None,
        )?;
        Ok(())
    }

    pub fn get_device_descriptor(&mut self, host: &mut dyn USBHost) -> Result<DeviceDescriptor, TransferError> {
        let mut dev_desc: DeviceDescriptor = DeviceDescriptor::default();
        self.control_get(host, DescriptorType::Device, 0, to_slice_mut(&mut dev_desc))?;
        if dev_desc.b_max_packet_size < self.control_ep.max_packet_size as u8 {
            self.control_ep.max_packet_size = dev_desc.b_max_packet_size as u16;
        }
        Ok(dev_desc)
    }


    pub fn get_configuration_descriptors(&mut self, host: &mut dyn USBHost, cfg_idx: u8, buffer: &mut [u8]) -> Result<usize, TransferError> {
        let mut config_root: ConfigurationDescriptor = ConfigurationDescriptor::default();
        self.control_get(host, DescriptorType::Configuration, cfg_idx, to_slice_mut(&mut config_root))?;
        if config_root.w_total_length as usize > buffer.len() {
            Err(TransferError::Permanent("Device config larger than buffer"))
        } else {
            self.control_get(host, DescriptorType::Configuration, cfg_idx, buffer)
        }
    }

    pub fn get_device_address(&self) -> Address {
        self.control_ep.device_address
    }

    pub fn set_device_address(&mut self, host: &mut dyn USBHost, dev_addr: Address) -> Result<(), TransferError> {
        if 0u8 == self.control_ep.device_address.into() {
            self.control_set(host, RequestCode::SetAddress, dev_addr.into())?;
            self.control_ep.device_address = dev_addr;
            Ok(())
        } else {
            Err(TransferError::Permanent("Device Address Already Set"))
        }
    }

    pub fn set_active_configuration(&mut self, host: &mut dyn USBHost, config_index: u8) -> Result<(), TransferError> {
        self.control_set(host, RequestCode::SetConfiguration, config_index)
    }

    pub fn set_active_interface(&mut self, host: &mut dyn USBHost, if_idx: u8) -> Result<(), TransferError> {
        self.control_set(host, RequestCode::SetInterface, if_idx)
    }
}


