//! Simple USB host-side driver for boot protocol keyboards.

use log::{self, error, info, trace, warn};
use usb_host::{
    ConfigurationDescriptor, DescriptorType, DeviceDescriptor, Direction, Driver, DriverError,
    Endpoint, EndpointDescriptor, InterfaceDescriptor, RequestCode, RequestDirection, RequestKind,
    RequestRecipient, RequestType, TransferError, TransferType, USBHost, WValue,
};

use core::convert::TryFrom;
use core::mem::{self, MaybeUninit};
use heapless::Vec;

// How long to wait before talking to the device again after setting
// its address. cf §9.2.6.3 of USB 2.0
const SETTLE_DELAY: usize = 2;

// How many total devices this driver can support.
const MAX_DEVICES: usize = 32;

// And how many endpoints we can support per-device.
const MAX_ENDPOINTS: usize = 2;

// The maximum size configuration descriptor we can handle.
const CONFIG_BUFFER_LEN: usize = 256;

/// Boot protocol keyboard driver for USB hosts.
#[derive(Default, Debug)]
pub struct MidiDriver {
    devices: Vec<Device, MAX_DEVICES>,
}

impl From<Device> for DriverError {
    fn from(dev: Device) -> Self {
        DriverError::Permanent(dev.addr, "Out of devices")
    }
}

impl From<MidiEndpoint> for TransferError {
    fn from(_: MidiEndpoint) -> Self {
        TransferError::Permanent("")
    }
}

impl Driver for MidiDriver {
    fn want_device(&self, ddesc: &DeviceDescriptor) -> bool {
        ddesc.b_device_class == USB_AUDIO_CLASS && ddesc.b_device_sub_class == USB_MIDI_STREAMING_SUBCLASS
    }

    fn add_device(&mut self, device: DeviceDescriptor, address: u8) -> Result<(), DriverError> {
        self.devices.push(Device::new(address, device.b_max_packet_size))?;
        Ok(())
    }

    fn remove_device(&mut self, address: u8) {
        if let Some((num, _dd)) = self.devices.iter().enumerate().find(|(_num, dd)| dd.addr == address) {
            self.devices.swap_remove(num);
        }
    }

    fn tick(&mut self, millis: usize, host: &mut dyn USBHost) -> Result<(), DriverError> {
        for dev in self.devices.iter_mut() {
            rprintln!("MIDI host dev: {:?}", dev);
            if let Err(TransferError::Permanent(e)) = dev.tick(millis, host) {
                return Err(DriverError::Permanent(dev.addr, e));
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum DeviceState {
    Addressed,
    Settling(usize),
    GetConfig,
    SetConfig(u8),
    SetProtocol,
    SetIdle,
    SetReport,
    Running,
}

#[derive(Debug)]
struct Device {
    addr: u8,
    ep0: MidiEndpoint,
    endpoints: Vec<MidiEndpoint, MAX_ENDPOINTS>,
    state: DeviceState,
}


impl Device {
    fn new(addr: u8, max_packet_size: u8) -> Self {
        Self {
            addr,
            ep0: MidiEndpoint::new(addr, 0, 0, TransferType::Control, Direction::In, u16::from(max_packet_size)),
            endpoints: Vec::new(),
            state: DeviceState::Addressed,
        }
    }


    fn tick(&mut self, millis: usize, host: &mut dyn USBHost /* callback: &mut dyn FnMut(u8, &[u8])*/) -> Result<(), TransferError> {
        // TODO: either we need another `control_transfer` that doesn't take data,
        // or this `none` value needs to be put in the usb-host layer. None of these options are good.
        // let none_u8: Option<&mut [u8]> = None;
        unsafe {
            static mut LAST_STATE: DeviceState = DeviceState::Addressed;
            if LAST_STATE != self.state {
                info!("{:?} -> {:?}", LAST_STATE, self.state);
                LAST_STATE = self.state;
            }
        }

        match self.state {
            DeviceState::Addressed => {
                self.state = DeviceState::Settling(millis + SETTLE_DELAY)
            }

            DeviceState::Settling(until) if millis < until => {
                // still waiting for device to settle
            }

            DeviceState::Settling(_settled) => {
                // TODO: This seems unnecessary. We're not using the device descriptor at all.
                let mut dev_desc: MaybeUninit<DeviceDescriptor> = MaybeUninit::uninit();
                let buf = unsafe { to_slice_mut(&mut dev_desc) };
                let len = host.control_transfer(
                    &mut self.ep0,
                    RequestType::from((
                        RequestDirection::DeviceToHost,
                        RequestKind::Standard,
                        RequestRecipient::Device,
                    )),
                    RequestCode::GetDescriptor,
                    WValue::from((0, DescriptorType::Device as u8)),
                    0,
                    Some(buf),
                )?;
                assert_eq!(len, mem::size_of::<DeviceDescriptor>());
                self.state = DeviceState::GetConfig
            }

            DeviceState::GetConfig => {
                let mut conf_desc: MaybeUninit<ConfigurationDescriptor> = MaybeUninit::uninit();
                let desc_buf = unsafe { to_slice_mut(&mut conf_desc) };
                let len = host.control_transfer(
                    &mut self.ep0,
                    RequestType::from((
                        RequestDirection::DeviceToHost,
                        RequestKind::Standard,
                        RequestRecipient::Device,
                    )),
                    RequestCode::GetDescriptor,
                    WValue::from((0, DescriptorType::Configuration as u8)),
                    0,
                    Some(desc_buf),
                )?;
                assert_eq!(len, mem::size_of::<ConfigurationDescriptor>());
                let conf_desc = unsafe { conf_desc.assume_init() };

                if (conf_desc.w_total_length as usize) > CONFIG_BUFFER_LEN {
                    trace!("config descriptor: {:?}", conf_desc);
                    return Err(TransferError::Permanent("Config descriptor too large"));
                }

                // TODO Use allocation?
                let mut config = [0 ; CONFIG_BUFFER_LEN];
                let config_buf = &mut config[..conf_desc.w_total_length as usize];
                let len = host.control_transfer(
                    &mut self.ep0,
                    RequestType::from((
                        RequestDirection::DeviceToHost,
                        RequestKind::Standard,
                        RequestRecipient::Device,
                    )),
                    RequestCode::GetDescriptor,
                    WValue::from((0, DescriptorType::Configuration as u8)),
                    0,
                    Some(config_buf),
                )?;
                assert_eq!(len, conf_desc.w_total_length as usize);
                let (interface_num, ep) = ep_for_midi_class(config_buf).expect("No MIDI device found");
                info!("MIDI device found on {:?}", ep);

                self.endpoints.push(MidiEndpoint::new(
                    self.addr,
                    ep.b_endpoint_address & 0x7f,
                    interface_num,
                    TransferType::Interrupt,
                    Direction::In,
                    ep.w_max_packet_size,
                ))?;

                // TODO Browse configs and pick the "best" one
                self.state = DeviceState::SetConfig(1)
            }

            DeviceState::SetConfig(config_index) => {
                host.control_transfer(
                    &mut self.ep0,
                    RequestType::from((
                        RequestDirection::HostToDevice,
                        RequestKind::Standard,
                        RequestRecipient::Device,
                    )),
                    RequestCode::SetConfiguration,
                    WValue::from((config_index, 0)),
                    0,
                    None,
                )?;
                self.state = DeviceState::SetProtocol;
            }

            DeviceState::SetProtocol => {
                if let Some(ep) = self.endpoints.get(0) {
                    host.control_transfer(
                        &mut self.ep0,
                        RequestType::from((
                            RequestDirection::HostToDevice,
                            RequestKind::Class,
                            RequestRecipient::Interface,
                        )),
                        RequestCode::SetInterface,
                        WValue::from((0, 0)),
                        u16::from(ep.interface_num),
                        None,
                    )?;

                    self.state = DeviceState::SetIdle;
                } else {
                    return Err(TransferError::Permanent("MIDI device has no endpoint"));
                }
            }

            DeviceState::SetIdle => {
                host.control_transfer(
                    &mut self.ep0,
                    RequestType::from((
                        RequestDirection::HostToDevice,
                        RequestKind::Class,
                        RequestRecipient::Interface,
                    )),
                    RequestCode::GetInterface,
                    WValue::from((0, 0)),
                    0,
                    None,
                )?;
                self.state = DeviceState::SetReport;
            }

            DeviceState::SetReport => {
                if let Some(ref mut ep) = self.endpoints.get(0) {
                    let mut r: [u8; 1] = [0];
                    let report = &mut r[..];
                    let res = host.control_transfer(
                        &mut self.ep0,
                        RequestType::from((
                            RequestDirection::HostToDevice,
                            RequestKind::Class,
                            RequestRecipient::Interface,
                        )),
                        RequestCode::SetConfiguration,
                        WValue::from((0, 2)),
                        u16::from(ep.interface_num),
                        Some(report),
                    );

                    if let Err(e) = res {
                        warn!("Couldn't set report: {:?}", e)
                    }

                    self.state = DeviceState::Running
                } else {
                    return Err(TransferError::Permanent("MIDI device has no endpoint"));
                }
            }

            DeviceState::Running => {
                if let Some(ep) = self.endpoints.get_mut(0) {
                    let mut b: [u8; 8] = [0; 8];
                    let buf = &mut b[..];
                    match host.in_transfer(ep, buf) {
                        Err(TransferError::Permanent(msg)) => {
                            error!("reading report: {}", msg);
                            return Err(TransferError::Permanent(msg));
                        }
                        Err(TransferError::Retry(_)) => return Ok(()),
                        Ok(_) => {
                            // callback(self.addr, buf);
                        }
                    }
                } else {
                    return Err(TransferError::Permanent("MIDI device has no endpoint"));
                }
            }
        }

        Ok(())
    }
}

unsafe fn to_slice_mut<T>(v: &mut T) -> &mut [u8] {
    let ptr = v as *mut T as *mut u8;
    let len = mem::size_of::<T>();
    core::slice::from_raw_parts_mut(ptr, len)
}

#[derive(Debug)]
struct MidiEndpoint {
    addr: u8,
    num: u8,
    interface_num: u8,
    transfer_type: TransferType,
    direction: Direction,
    max_packet_size: u16,
    in_toggle: bool,
    out_toggle: bool,
}

impl MidiEndpoint {
    fn new(addr: u8, num: u8, interface_num: u8, transfer_type: TransferType, direction: Direction, max_packet_size: u16) -> Self {
        Self {
            addr,
            num,
            interface_num,
            transfer_type,
            direction,
            max_packet_size,
            in_toggle: false,
            out_toggle: false,
        }
    }
}

impl Endpoint for MidiEndpoint {
    fn address(&self) -> u8 {
        self.addr
    }

    fn endpoint_num(&self) -> u8 {
        self.num
    }

    fn transfer_type(&self) -> TransferType {
        self.transfer_type
    }

    fn direction(&self) -> Direction {
        self.direction
    }

    fn max_packet_size(&self) -> u16 {
        self.max_packet_size
    }

    fn in_toggle(&self) -> bool {
        self.in_toggle
    }

    fn set_in_toggle(&mut self, toggle: bool) {
        self.in_toggle = toggle
    }

    fn out_toggle(&self) -> bool {
        self.out_toggle
    }

    fn set_out_toggle(&mut self, toggle: bool) {
        self.out_toggle = toggle
    }
}

enum Descriptor<'a> {
    Configuration(&'a ConfigurationDescriptor),
    Interface(&'a InterfaceDescriptor),
    Endpoint(&'a EndpointDescriptor),
    Other(&'a [u8]),
}

// TODO Iter impl.
struct DescriptorParser<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> From<&'a [u8]> for DescriptorParser<'a> {
    fn from(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
}

impl<'a> DescriptorParser<'a> {
    fn next<'b>(&'b mut self) -> Option<Descriptor<'a>> {
        if self.pos == self.buf.len() {
            return None;
        }

        assert!(self.pos < (i32::max_value() as usize));
        assert!(self.pos <= self.buf.len() + 2);

        let end = self.pos + self.buf[self.pos] as usize;
        assert!(end <= self.buf.len());

        // TODO: this is basically guaranteed to have unaligned access, isn't it? That's not good. RIP zero-copy?
        let res = match DescriptorType::try_from(self.buf[self.pos + 1]) {
            Ok(DescriptorType::Configuration) => {
                let desc: &ConfigurationDescriptor = unsafe {
                    let ptr = self.buf.as_ptr().add(self.pos);
                    &*(ptr as *const _)
                };
                Some(Descriptor::Configuration(desc))
            }

            Ok(DescriptorType::Interface) => {
                let desc: &InterfaceDescriptor = unsafe {
                    let ptr = self.buf.as_ptr().add(self.pos);
                    &*(ptr as *const _)
                };
                Some(Descriptor::Interface(desc))
            }

            Ok(DescriptorType::Endpoint) => {
                let desc: &EndpointDescriptor = unsafe {
                    let ptr = self.buf.as_ptr().add(self.pos);
                    &*(ptr as *const _)
                };
                Some(Descriptor::Endpoint(desc))
            }

            // Return a raw byte slice if we don't know how to parse the descriptor naturally, so callers can figure it out.
            Err(_) => Some(Descriptor::Other(&self.buf[self.pos..end])),
            _ => Some(Descriptor::Other(&self.buf[self.pos..end])),
        };

        self.pos = end;
        res
    }
}

pub const USB_MIDI_PACKET_LEN: usize = 4;

pub const USB_CLASS_NONE: u8 = 0x00;
pub const USB_AUDIO_CLASS: u8 = 0x01;
pub const USB_AUDIO_CONTROL_SUBCLASS: u8 = 0x01;
pub const USB_MIDI_STREAMING_SUBCLASS: u8 = 0x03;

/// If a midi device is found, return its interface number and endpoint.
fn ep_for_midi_class(buf: &[u8]) -> Option<(u8, &EndpointDescriptor)> {
    let mut parser = DescriptorParser::from(buf);
    let mut midi_interface = None;
    while let Some(desc) = parser.next() {
        match desc {
            Descriptor::Interface(idesc)
            if idesc.b_interface_class == USB_AUDIO_CLASS
                && idesc.b_interface_sub_class == USB_MIDI_STREAMING_SUBCLASS
                && idesc.b_interface_protocol == 0x00 =>
                midi_interface = Some(idesc.b_interface_number),

            Descriptor::Interface(_) => {
                midi_interface = None;
                info!("Ignoring non-MIDI device")
            }

            Descriptor::Endpoint(edesc) =>
                if let Some(interface_num) = midi_interface {
                    return Some((interface_num, edesc));
                }

            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn add_remove_device() {
        let mut driver = MidiDriver::new(|_addr, _report| {});

        let count = |driver: &mut MidiDriver<_>| {
            driver
                .devices
                .iter()
                .fold(0, |sum, dev| sum + dev.as_ref().map_or(0, |_| 1))
        };
        assert_eq!(count(&mut driver), 0);

        driver.add_device(dummy_device(), 2).unwrap();
        assert_eq!(count(&mut driver), 1);

        driver.remove_device(2);
        assert_eq!(count(&mut driver), 0);
    }

    #[test]
    fn too_many_devices() {
        let mut driver = MidiDriver::new(|_addr, _report| {});

        for i in 0..MAX_DEVICES {
            driver.add_device(dummy_device(), (i + 1) as u8).unwrap();
        }
        assert!(driver
            .add_device(dummy_device(), (MAX_DEVICES + 1) as u8)
            .is_err());
    }

    #[test]
    fn tick_propagates_errors() {
        let mut dummyhost = DummyHost { fail: true };

        let mut calls = 0;
        let mut driver = MidiDriver::new(|_addr, _report| calls += 1);

        driver.add_device(dummy_device(), 1).unwrap();
        driver.tick(0, &mut dummyhost).unwrap();
        assert!(driver.tick(SETTLE_DELAY + 1, &mut dummyhost).is_err());
    }

    fn dummy_device() -> DeviceDescriptor {
        DeviceDescriptor {
            b_length: mem::size_of::<DeviceDescriptor>() as u8,
            b_descriptor_type: DescriptorType::Device,
            bcd_usb: 0x0110,
            b_device_class: 0,
            b_device_sub_class: 0,
            b_device_protocol: 0,
            b_max_packet_size: 8,
            id_vendor: 0xdead,
            id_product: 0xbeef,
            bcd_device: 0xf00d,
            i_manufacturer: 1,
            i_product: 2,
            i_serial_number: 3,
            b_num_configurations: 1,
        }
    }

    #[test]
    fn parse_keyboardio_config() {
        let raw: &[u8] = &[
            0x09, 0x02, 0x96, 0x00, 0x05, 0x01, 0x00, 0xa0, 0xfa, 0x08, 0x0b, 0x00, 0x02, 0x02,
            0x02, 0x01, 0x00, 0x09, 0x04, 0x00, 0x00, 0x01, 0x02, 0x02, 0x00, 0x00, 0x05, 0x24,
            0x00, 0x10, 0x01, 0x05, 0x24, 0x01, 0x01, 0x01, 0x04, 0x24, 0x02, 0x06, 0x05, 0x24,
            0x06, 0x00, 0x01, 0x07, 0x05, 0x81, 0x03, 0x10, 0x00, 0x40, 0x09, 0x04, 0x01, 0x00,
            0x02, 0x0a, 0x00, 0x00, 0x00, 0x07, 0x05, 0x02, 0x02, 0x40, 0x00, 0x00, 0x07, 0x05,
            0x83, 0x02, 0x40, 0x00, 0x00, 0x09, 0x04, 0x02, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00,
            0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x35, 0x00, 0x07, 0x05, 0x84, 0x03, 0x40,
            0x00, 0x01, 0x09, 0x04, 0x03, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00, 0x09, 0x21, 0x01,
            0x01, 0x00, 0x01, 0x22, 0x72, 0x00, 0x07, 0x05, 0x85, 0x03, 0x40, 0x00, 0x01, 0x09,
            0x04, 0x04, 0x00, 0x01, 0x03, 0x01, 0x01, 0x00, 0x09, 0x21, 0x01, 0x01, 0x00, 0x01,
            0x22, 0x3f, 0x00, 0x07, 0x05, 0x86, 0x03, 0x40, 0x00, 0x01,
        ];
        let mut parser = DescriptorParser::from(raw);

        let config_desc = ConfigurationDescriptor {
            b_length: 9,
            b_descriptor_type: DescriptorType::Configuration,
            w_total_length: 150,
            b_num_interfaces: 5,
            b_configuration_value: 1,
            i_configuration: 0,
            bm_attributes: 0xa0,
            b_max_power: 250,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Configuration(cdesc) = desc {
            assert_eq!(*cdesc, config_desc, "Configuration descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // Interface Association Descriptor
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Other(odesc) = desc {
            let odesc1: &[u8] = &[0x08, 0x0b, 0x00, 0x02, 0x02, 0x02, 0x01, 0x00];
            assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
        } else {
            panic!("Wrong descriptor type.")
        }

        let interface_desc1 = InterfaceDescriptor {
            b_length: 9,
            b_descriptor_type: DescriptorType::Interface,
            b_interface_number: 0,
            b_alternate_setting: 0,
            b_num_endpoints: 1,
            b_interface_class: 0x02,     // Communications and CDC Control
            b_interface_sub_class: 0x02, // Abstract Control Model
            b_interface_protocol: 0x00,
            i_interface: 0,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Interface(cdesc) = desc {
            assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // Four communications descriptors.
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Other(odesc) = desc {
            let odesc1: &[u8] = &[0x05, 0x24, 0x00, 0x10, 0x01];
            assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
        } else {
            panic!("Wrong descriptor type.")
        }

        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Other(odesc) = desc {
            let odesc1: &[u8] = &[0x05, 0x24, 0x01, 0x01, 0x01];
            assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
        } else {
            panic!("Wrong descriptor type.")
        }

        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Other(odesc) = desc {
            let odesc1: &[u8] = &[0x04, 0x24, 0x02, 0x06];
            assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
        } else {
            panic!("Wrong descriptor type.")
        }

        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Other(odesc) = desc {
            let odesc1: &[u8] = &[0x05, 0x24, 0x06, 0x00, 0x01];
            assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
        } else {
            panic!("Wrong descriptor type.")
        }

        let endpoint_desc1 = EndpointDescriptor {
            b_length: 7,
            b_descriptor_type: DescriptorType::Endpoint,
            b_endpoint_address: 0x81,
            bm_attributes: 0x03,
            w_max_packet_size: 16,
            b_interval: 64,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Endpoint(cdesc) = desc {
            assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // CDC-Data interface.
        let interface_desc1 = InterfaceDescriptor {
            b_length: 9,
            b_descriptor_type: DescriptorType::Interface,
            b_interface_number: 1,
            b_alternate_setting: 0,
            b_num_endpoints: 2,
            b_interface_class: 0x0a, // CDC-Data
            b_interface_sub_class: 0x00,
            b_interface_protocol: 0x00,
            i_interface: 0,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Interface(cdesc) = desc {
            assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        let endpoint_desc1 = EndpointDescriptor {
            b_length: 7,
            b_descriptor_type: DescriptorType::Endpoint,
            b_endpoint_address: 0x02,
            bm_attributes: 0x02,
            w_max_packet_size: 64,
            b_interval: 0,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Endpoint(cdesc) = desc {
            assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        let endpoint_desc1 = EndpointDescriptor {
            b_length: 7,
            b_descriptor_type: DescriptorType::Endpoint,
            b_endpoint_address: 0x83,
            bm_attributes: 0x02,
            w_max_packet_size: 64,
            b_interval: 0,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Endpoint(cdesc) = desc {
            assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // HID interface.
        let interface_desc1 = InterfaceDescriptor {
            b_length: 9,
            b_descriptor_type: DescriptorType::Interface,
            b_interface_number: 2,
            b_alternate_setting: 0,
            b_num_endpoints: 1,
            b_interface_class: 0x03, // HID
            b_interface_sub_class: 0x00,
            b_interface_protocol: 0x00,
            i_interface: 0,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Interface(cdesc) = desc {
            assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // HID Descriptor.
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Other(odesc) = desc {
            let odesc1: &[u8] = &[0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x35, 0x00];
            assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
        } else {
            panic!("Wrong descriptor type.")
        }

        let endpoint_desc1 = EndpointDescriptor {
            b_length: 7,
            b_descriptor_type: DescriptorType::Endpoint,
            b_endpoint_address: 0x84,
            bm_attributes: 0x03,
            w_max_packet_size: 64,
            b_interval: 1,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Endpoint(cdesc) = desc {
            assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // HID interface.
        let interface_desc1 = InterfaceDescriptor {
            b_length: 9,
            b_descriptor_type: DescriptorType::Interface,
            b_interface_number: 3,
            b_alternate_setting: 0,
            b_num_endpoints: 1,
            b_interface_class: 0x03, // HID
            b_interface_sub_class: 0x00,
            b_interface_protocol: 0x00,
            i_interface: 0,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Interface(cdesc) = desc {
            assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // HID Descriptor.
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Other(odesc) = desc {
            let odesc1: &[u8] = &[0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x72, 0x00];
            assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
        } else {
            panic!("Wrong descriptor type.")
        }

        let endpoint_desc1 = EndpointDescriptor {
            b_length: 7,
            b_descriptor_type: DescriptorType::Endpoint,
            b_endpoint_address: 0x85,
            bm_attributes: 0x03,
            w_max_packet_size: 64,
            b_interval: 1,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Endpoint(cdesc) = desc {
            assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // HID interface.
        let interface_desc1 = InterfaceDescriptor {
            b_length: 9,
            b_descriptor_type: DescriptorType::Interface,
            b_interface_number: 4,
            b_alternate_setting: 0,
            b_num_endpoints: 1,
            b_interface_class: 0x03,     // HID
            b_interface_sub_class: 0x01, // Boot Interface
            b_interface_protocol: 0x01,  // Keyboard
            i_interface: 0,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Interface(cdesc) = desc {
            assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        // HID Descriptor.
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Other(odesc) = desc {
            let odesc1: &[u8] = &[0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x3f, 0x00];
            assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
        } else {
            panic!("Wrong descriptor type.")
        }

        let endpoint_desc1 = EndpointDescriptor {
            b_length: 7,
            b_descriptor_type: DescriptorType::Endpoint,
            b_endpoint_address: 0x86,
            bm_attributes: 0x03,
            w_max_packet_size: 64,
            b_interval: 1,
        };
        let desc = parser.next().expect("Parsing configuration");
        if let Descriptor::Endpoint(cdesc) = desc {
            assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
        } else {
            panic!("Wrong descriptor type.");
        }

        assert!(parser.next().is_none(), "Extra descriptors.");
    }

    #[test]
    fn keyboardio_discovers_bootkbd() {
        let raw: &[u8] = &[
            0x09, 0x02, 0x96, 0x00, 0x05, 0x01, 0x00, 0xa0, 0xfa, 0x08, 0x0b, 0x00, 0x02, 0x02,
            0x02, 0x01, 0x00, 0x09, 0x04, 0x00, 0x00, 0x01, 0x02, 0x02, 0x00, 0x00, 0x05, 0x24,
            0x00, 0x10, 0x01, 0x05, 0x24, 0x01, 0x01, 0x01, 0x04, 0x24, 0x02, 0x06, 0x05, 0x24,
            0x06, 0x00, 0x01, 0x07, 0x05, 0x81, 0x03, 0x10, 0x00, 0x40, 0x09, 0x04, 0x01, 0x00,
            0x02, 0x0a, 0x00, 0x00, 0x00, 0x07, 0x05, 0x02, 0x02, 0x40, 0x00, 0x00, 0x07, 0x05,
            0x83, 0x02, 0x40, 0x00, 0x00, 0x09, 0x04, 0x02, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00,
            0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x35, 0x00, 0x07, 0x05, 0x84, 0x03, 0x40,
            0x00, 0x01, 0x09, 0x04, 0x03, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00, 0x09, 0x21, 0x01,
            0x01, 0x00, 0x01, 0x22, 0x72, 0x00, 0x07, 0x05, 0x85, 0x03, 0x40, 0x00, 0x01, 0x09,
            0x04, 0x04, 0x00, 0x01, 0x03, 0x01, 0x01, 0x00, 0x09, 0x21, 0x01, 0x01, 0x00, 0x01,
            0x22, 0x3f, 0x00, 0x07, 0x05, 0x86, 0x03, 0x40, 0x00, 0x01,
        ];

        let (got_inum, got) = ep_for_midi_class(raw).expect("Looking for endpoint");
        let want = EndpointDescriptor {
            b_length: 7,
            b_descriptor_type: DescriptorType::Endpoint,
            b_endpoint_address: 0x86,
            bm_attributes: 0x03,
            w_max_packet_size: 64,
            b_interval: 1,
        };
        assert_eq!(got_inum, 4);
        assert_eq!(*got, want);
    }

    struct DummyHost {
        fail: bool,
    }

    impl USBHost for DummyHost {
        fn control_transfer(
            &mut self,
            _ep: &mut dyn Endpoint,
            _bm_request_type: RequestType,
            _b_request: RequestCode,
            _w_value: WValue,
            _w_index: u16,
            _buf: Option<&mut [u8]>,
        ) -> Result<usize, TransferError> {
            if self.fail {
                Err(TransferError::Permanent("foo"))
            } else {
                Ok(0)
            }
        }

        fn in_transfer(
            &mut self,
            _ep: &mut dyn Endpoint,
            _buf: &mut [u8],
        ) -> Result<usize, TransferError> {
            if self.fail {
                Err(TransferError::Permanent("foo"))
            } else {
                Ok(0)
            }
        }

        fn out_transfer(
            &mut self,
            _ep: &mut dyn Endpoint,
            _buf: &[u8],
        ) -> Result<usize, TransferError> {
            if self.fail {
                Err(TransferError::Permanent("foo"))
            } else {
                Ok(0)
            }
        }
    }
}
