//! Simple USB host-side driver for boot protocol keyboards.

use core::fmt::{Debug, Formatter};
use atsamd_usb_host::usb_host::{
    DeviceDescriptor, Driver, DriverError,
    Endpoint, InterfaceDescriptor, RequestDirection, RequestKind,
    RequestRecipient, RequestType, TransferError, TransferType, USBHost, WValue,
};

use heapless::{FnvIndexMap, Vec};
use atsamd_usb_host::usb_host::address::Address;
use atsamd_usb_host::usb_host::device::Device;
use atsamd_usb_host::usb_host::parser::{DescriptorRef, DescriptorParser};
use midi::{Packet, PacketList, PacketParser, Receive, ReceiveListener, Transmit};
use sync_thumbv6m::alloc::Arc;
use sync_thumbv6m::array_queue::ArrayQueue;
use sync_thumbv6m::spin::SpinMutex;

// How long to wait before talking to the device again after setting
// its address. cf §9.2.6.3 of USB 2.0
const SETTLE_DELAY: u64 = 2;

// How many total devices this driver can support.
const MAX_DEVICES: usize = 32;

// And how many endpoints we can support per-device.
const MAX_ENDPOINTS: usize = 2;

pub const USB_MIDI_PACKET_LEN: usize = 4;

pub const USB_CLASS_NONE: u8 = 0x00;
pub const USB_AUDIO_CLASS: u8 = 0x01;
pub const USB_AUDIO_CONTROL_SUBCLASS: u8 = 0x01;
pub const USB_MIDI_STREAMING_SUBCLASS: u8 = 0x03;

fn is_midi_interface(idesc: &InterfaceDescriptor) -> bool {
    idesc.b_interface_class == USB_AUDIO_CLASS
        && idesc.b_interface_sub_class == USB_MIDI_STREAMING_SUBCLASS
        && idesc.b_interface_protocol == 0x00
}

const MAX_PORTS: usize = 8;

static mut USB_MIDI_PORTS: heapless::FnvIndexMap<(u8, Address), UsbMidiPort, MAX_PORTS> = FnvIndexMap::new();

#[derive(Debug)]
struct UsbMidiPort {
    input: Option<UsbMidiReadPort>,
    output: Option<UsbMidiWritePort>,
}
// = FnvIndexMap::new();
/// Boot protocol keyboard driver for USB hosts.
#[derive(Default, Debug)]
pub struct MidiDriver {
     devices: heapless::FnvIndexMap<(u8, Address), UsbMidiPort, MAX_PORTS>
}

#[derive(Debug)]
pub struct UsbMidiWritePort {
    ep: Endpoint,
    buffer: ArrayQueue<Packet, 17>,
}

impl Transmit for UsbMidiWritePort {
    fn transmit(&mut self, events: embedded_midi::PacketList) -> Result<(), embedded_midi::MidiError> {
        // FIXME PacketList should implement IntoIterator or just be simplified
        for p in &*events {
            self.buffer.push(p);
        }
        Ok(())
    }
}

pub struct UsbMidiReadPort {
    ep: Endpoint,
    cb: SpinMutex<Option<&'static mut (dyn FnMut(PacketList) + Send + Sync)>>,
}

impl Debug for UsbMidiReadPort {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        self.ep.fmt(f)
    }
}

// impl Receive for UsbMidiReadPort {
//     fn receive(&mut self) -> Result<Option<embedded_midi::Packet>, embedded_midi::MidiError> {
//         todo!()
//     }
// }

impl ReceiveListener for UsbMidiReadPort {
    fn on_receive(&mut self, listener: Option<&'static mut (dyn FnMut(PacketList) + Send + Sync)>) {
        *self.cb.lock() = listener
    }
}

impl Driver for MidiDriver {
    fn connected(&mut self, host: &mut dyn USBHost, device: &mut Device, device_desc: &DeviceDescriptor, config_descriptors: &mut DescriptorParser) -> Result<bool, TransferError> {
        let mut midi_interface = None;
        let mut eps: FnvIndexMap<u8, UsbMidiPort, 4> = FnvIndexMap::new();
        while let Some(desc) = config_descriptors.next() {
            match desc {
                DescriptorRef::Interface(idesc) if is_midi_interface(idesc) =>
                    midi_interface = Some(idesc),

                DescriptorRef::Interface(_) => if midi_interface.is_some() {
                    // new interface, done enumerating MIDI endpoints
                    break;
                }

                DescriptorRef::Endpoint(edesc) =>
                    if let Some(interface_num) = midi_interface {
                        let abs_port = edesc.b_endpoint_address & 0xF;

                        let mut port = eps.get_mut(&(abs_port));
                        if port.is_none() {
                            // FIXME rewrite to use entry().or_insert() when heapless implements it
                            let new_port = UsbMidiPort { input: None, output: None };
                            eps.insert(abs_port, new_port);
                            port = eps.get_mut(&(abs_port));
                        }
                        // we've made sure
                        if let Some(port) = port {
                            let ep = device.endpoint(edesc);
                            if edesc.b_endpoint_address & 0x80 == 0 {
                                if port.output.is_none() {
                                    port.output = Some(UsbMidiWritePort { ep, buffer: ArrayQueue::new() })
                                } else {
                                    warn!("USB MIDI Output Port already defined")
                                }
                            } else {
                                if port.input.is_none() {
                                    port.input = Some(UsbMidiReadPort { ep, cb: SpinMutex::new(None) })
                                } else {
                                    warn!("USB MIDI Input Port already defined")
                                }
                            }
                        }
                    }
                _ => {}
            }
        }
        if let Some(midi_if) = midi_interface {
            device.set_active_interface(host, midi_if.b_interface_number)?;
            // FIXME use drain() when heapless implements it
            let mut keys: Vec<u8, 4> = Vec::new();

            for k in eps.keys() {
                keys.push(*k);
            }
            for k in keys {
                let v = eps.remove(&k).unwrap();
                unsafe { USB_MIDI_PORTS.insert((host.get_host_id(), device.get_device_address()), v) };
                // TODO more port indexes (by name, by id, etc.)
            }
        }
        Ok(false)
    }

    fn disconnected(&mut self, host: &mut dyn USBHost, device: &mut Device) {
        unsafe { USB_MIDI_PORTS.remove(&(host.get_host_id(), device.get_device_address())) };
        // TODO clear other indexes (by name, by id, etc.)
    }

    fn tick(&mut self, host: &mut dyn USBHost) -> Result<(), DriverError> {
        for port in unsafe { &mut USB_MIDI_PORTS }.values_mut() {
            if let Some(output) = &mut port.output {
                while let Some(packet) = output.buffer.pop() {
                    // TODO send all packets at once
                    host.out_transfer(&mut output.ep, packet.payload());
                }
            }
            if let Some(input) = &mut port.input {
                let mut buf = [0; 64];
                if let Ok(len) = host.in_transfer(&mut input.ep, &mut buf) {
                    if len > 0 {
                        let mut callback = input.cb.lock();
                        if let Some(mut callback) = callback.as_mut() {
                            let mut pp = PacketParser::default();
                            for b in &buf[..len] {
                                match pp.advance(*b) {
                                    // TODO receive all packets at once
                                    Ok(Some(packet)) => (callback)(PacketList::single(packet)),
                                    Err(e) => warn!("USB MIDI Packet Error{:?}", e),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}


// #[cfg(test)]
// mod test {
//     use super::*;
//
//     #[test]
//     fn add_remove_device() {
//         let mut driver = MidiDriver::new(|_addr, _report| {});
//
//         let count = |driver: &mut MidiDriver<_>| {
//             driver
//                 .devices
//                 .iter()
//                 .fold(0, |sum, dev| sum + dev.as_ref().map_or(0, |_| 1))
//         };
//         assert_eq!(count(&mut driver), 0);
//
//         driver.add_device(dummy_device(), 2).unwrap();
//         assert_eq!(count(&mut driver), 1);
//
//         driver.remove_device(2);
//         assert_eq!(count(&mut driver), 0);
//     }
//
//     #[test]
//     fn too_many_devices() {
//         let mut driver = MidiDriver::new(|_addr, _report| {});
//
//         for i in 0..MAX_DEVICES {
//             driver.add_device(dummy_device(), (i + 1) as u8).unwrap();
//         }
//         assert!(driver
//             .add_device(dummy_device(), (MAX_DEVICES + 1) as u8)
//             .is_err());
//     }
//
//     #[test]
//     fn tick_propagates_errors() {
//         let mut dummyhost = DummyHost { fail: true };
//
//         let mut calls = 0;
//         let mut driver = MidiDriver::new(|_addr, _report| calls += 1);
//
//         driver.add_device(dummy_device(), 1).unwrap();
//         driver.tick(0, &mut dummyhost).unwrap();
//         assert!(driver.tick(SETTLE_DELAY + 1, &mut dummyhost).is_err());
//     }
//
//     fn dummy_device() -> DeviceDescriptor {
//         DeviceDescriptor {
//             b_length: mem::size_of::<DeviceDescriptor>() as u8,
//             b_descriptor_type: DescriptorType::Device,
//             bcd_usb: 0x0110,
//             b_device_class: 0,
//             b_device_sub_class: 0,
//             b_device_protocol: 0,
//             b_max_packet_size: 8,
//             id_vendor: 0xdead,
//             id_product: 0xbeef,
//             bcd_device: 0xf00d,
//             i_manufacturer: 1,
//             i_product: 2,
//             i_serial_number: 3,
//             b_num_configurations: 1,
//         }
//     }
//
//     #[test]
//     fn parse_keyboardio_config() {
//         let raw: &[u8] = &[
//             0x09, 0x02, 0x96, 0x00, 0x05, 0x01, 0x00, 0xa0, 0xfa, 0x08, 0x0b, 0x00, 0x02, 0x02,
//             0x02, 0x01, 0x00, 0x09, 0x04, 0x00, 0x00, 0x01, 0x02, 0x02, 0x00, 0x00, 0x05, 0x24,
//             0x00, 0x10, 0x01, 0x05, 0x24, 0x01, 0x01, 0x01, 0x04, 0x24, 0x02, 0x06, 0x05, 0x24,
//             0x06, 0x00, 0x01, 0x07, 0x05, 0x81, 0x03, 0x10, 0x00, 0x40, 0x09, 0x04, 0x01, 0x00,
//             0x02, 0x0a, 0x00, 0x00, 0x00, 0x07, 0x05, 0x02, 0x02, 0x40, 0x00, 0x00, 0x07, 0x05,
//             0x83, 0x02, 0x40, 0x00, 0x00, 0x09, 0x04, 0x02, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00,
//             0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x35, 0x00, 0x07, 0x05, 0x84, 0x03, 0x40,
//             0x00, 0x01, 0x09, 0x04, 0x03, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00, 0x09, 0x21, 0x01,
//             0x01, 0x00, 0x01, 0x22, 0x72, 0x00, 0x07, 0x05, 0x85, 0x03, 0x40, 0x00, 0x01, 0x09,
//             0x04, 0x04, 0x00, 0x01, 0x03, 0x01, 0x01, 0x00, 0x09, 0x21, 0x01, 0x01, 0x00, 0x01,
//             0x22, 0x3f, 0x00, 0x07, 0x05, 0x86, 0x03, 0x40, 0x00, 0x01,
//         ];
//         let mut parser = DescriptorParser::from(raw);
//
//         let config_desc = ConfigurationDescriptor {
//             b_length: 9,
//             b_descriptor_type: DescriptorType::Configuration,
//             w_total_length: 150,
//             b_num_interfaces: 5,
//             b_configuration_value: 1,
//             i_configuration: 0,
//             bm_attributes: 0xa0,
//             b_max_power: 250,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Configuration(cdesc) = desc {
//             assert_eq!(*cdesc, config_desc, "Configuration descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // Interface Association Descriptor
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Other(odesc) = desc {
//             let odesc1: &[u8] = &[0x08, 0x0b, 0x00, 0x02, 0x02, 0x02, 0x01, 0x00];
//             assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
//         } else {
//             panic!("Wrong descriptor type.")
//         }
//
//         let interface_desc1 = InterfaceDescriptor {
//             b_length: 9,
//             b_descriptor_type: DescriptorType::Interface,
//             b_interface_number: 0,
//             b_alternate_setting: 0,
//             b_num_endpoints: 1,
//             b_interface_class: 0x02,     // Communications and CDC Control
//             b_interface_sub_class: 0x02, // Abstract Control Model
//             b_interface_protocol: 0x00,
//             i_interface: 0,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Interface(cdesc) = desc {
//             assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // Four communications descriptors.
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Other(odesc) = desc {
//             let odesc1: &[u8] = &[0x05, 0x24, 0x00, 0x10, 0x01];
//             assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
//         } else {
//             panic!("Wrong descriptor type.")
//         }
//
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Other(odesc) = desc {
//             let odesc1: &[u8] = &[0x05, 0x24, 0x01, 0x01, 0x01];
//             assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
//         } else {
//             panic!("Wrong descriptor type.")
//         }
//
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Other(odesc) = desc {
//             let odesc1: &[u8] = &[0x04, 0x24, 0x02, 0x06];
//             assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
//         } else {
//             panic!("Wrong descriptor type.")
//         }
//
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Other(odesc) = desc {
//             let odesc1: &[u8] = &[0x05, 0x24, 0x06, 0x00, 0x01];
//             assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
//         } else {
//             panic!("Wrong descriptor type.")
//         }
//
//         let endpoint_desc1 = EndpointDescriptor {
//             b_length: 7,
//             b_descriptor_type: DescriptorType::Endpoint,
//             b_endpoint_address: 0x81,
//             bm_attributes: 0x03,
//             w_max_packet_size: 16,
//             b_interval: 64,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Endpoint(cdesc) = desc {
//             assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // CDC-Data interface.
//         let interface_desc1 = InterfaceDescriptor {
//             b_length: 9,
//             b_descriptor_type: DescriptorType::Interface,
//             b_interface_number: 1,
//             b_alternate_setting: 0,
//             b_num_endpoints: 2,
//             b_interface_class: 0x0a, // CDC-Data
//             b_interface_sub_class: 0x00,
//             b_interface_protocol: 0x00,
//             i_interface: 0,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Interface(cdesc) = desc {
//             assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         let endpoint_desc1 = EndpointDescriptor {
//             b_length: 7,
//             b_descriptor_type: DescriptorType::Endpoint,
//             b_endpoint_address: 0x02,
//             bm_attributes: 0x02,
//             w_max_packet_size: 64,
//             b_interval: 0,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Endpoint(cdesc) = desc {
//             assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         let endpoint_desc1 = EndpointDescriptor {
//             b_length: 7,
//             b_descriptor_type: DescriptorType::Endpoint,
//             b_endpoint_address: 0x83,
//             bm_attributes: 0x02,
//             w_max_packet_size: 64,
//             b_interval: 0,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Endpoint(cdesc) = desc {
//             assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // HID interface.
//         let interface_desc1 = InterfaceDescriptor {
//             b_length: 9,
//             b_descriptor_type: DescriptorType::Interface,
//             b_interface_number: 2,
//             b_alternate_setting: 0,
//             b_num_endpoints: 1,
//             b_interface_class: 0x03, // HID
//             b_interface_sub_class: 0x00,
//             b_interface_protocol: 0x00,
//             i_interface: 0,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Interface(cdesc) = desc {
//             assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // HID Descriptor.
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Other(odesc) = desc {
//             let odesc1: &[u8] = &[0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x35, 0x00];
//             assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
//         } else {
//             panic!("Wrong descriptor type.")
//         }
//
//         let endpoint_desc1 = EndpointDescriptor {
//             b_length: 7,
//             b_descriptor_type: DescriptorType::Endpoint,
//             b_endpoint_address: 0x84,
//             bm_attributes: 0x03,
//             w_max_packet_size: 64,
//             b_interval: 1,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Endpoint(cdesc) = desc {
//             assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // HID interface.
//         let interface_desc1 = InterfaceDescriptor {
//             b_length: 9,
//             b_descriptor_type: DescriptorType::Interface,
//             b_interface_number: 3,
//             b_alternate_setting: 0,
//             b_num_endpoints: 1,
//             b_interface_class: 0x03, // HID
//             b_interface_sub_class: 0x00,
//             b_interface_protocol: 0x00,
//             i_interface: 0,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Interface(cdesc) = desc {
//             assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // HID Descriptor.
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Other(odesc) = desc {
//             let odesc1: &[u8] = &[0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x72, 0x00];
//             assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
//         } else {
//             panic!("Wrong descriptor type.")
//         }
//
//         let endpoint_desc1 = EndpointDescriptor {
//             b_length: 7,
//             b_descriptor_type: DescriptorType::Endpoint,
//             b_endpoint_address: 0x85,
//             bm_attributes: 0x03,
//             w_max_packet_size: 64,
//             b_interval: 1,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Endpoint(cdesc) = desc {
//             assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // HID interface.
//         let interface_desc1 = InterfaceDescriptor {
//             b_length: 9,
//             b_descriptor_type: DescriptorType::Interface,
//             b_interface_number: 4,
//             b_alternate_setting: 0,
//             b_num_endpoints: 1,
//             b_interface_class: 0x03,     // HID
//             b_interface_sub_class: 0x01, // Boot Interface
//             b_interface_protocol: 0x01,  // Keyboard
//             i_interface: 0,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Interface(cdesc) = desc {
//             assert_eq!(*cdesc, interface_desc1, "Interface descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         // HID Descriptor.
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Other(odesc) = desc {
//             let odesc1: &[u8] = &[0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x3f, 0x00];
//             assert_eq!(odesc, odesc1, "Interface descriptor mismatch");
//         } else {
//             panic!("Wrong descriptor type.")
//         }
//
//         let endpoint_desc1 = EndpointDescriptor {
//             b_length: 7,
//             b_descriptor_type: DescriptorType::Endpoint,
//             b_endpoint_address: 0x86,
//             bm_attributes: 0x03,
//             w_max_packet_size: 64,
//             b_interval: 1,
//         };
//         let desc = parser.next().expect("Parsing configuration");
//         if let Descriptor::Endpoint(cdesc) = desc {
//             assert_eq!(*cdesc, endpoint_desc1, "Endpoint descriptor mismatch.");
//         } else {
//             panic!("Wrong descriptor type.");
//         }
//
//         assert!(parser.next().is_none(), "Extra descriptors.");
//     }
//
//     #[test]
//     fn keyboardio_discovers_bootkbd() {
//         let raw: &[u8] = &[
//             0x09, 0x02, 0x96, 0x00, 0x05, 0x01, 0x00, 0xa0, 0xfa, 0x08, 0x0b, 0x00, 0x02, 0x02,
//             0x02, 0x01, 0x00, 0x09, 0x04, 0x00, 0x00, 0x01, 0x02, 0x02, 0x00, 0x00, 0x05, 0x24,
//             0x00, 0x10, 0x01, 0x05, 0x24, 0x01, 0x01, 0x01, 0x04, 0x24, 0x02, 0x06, 0x05, 0x24,
//             0x06, 0x00, 0x01, 0x07, 0x05, 0x81, 0x03, 0x10, 0x00, 0x40, 0x09, 0x04, 0x01, 0x00,
//             0x02, 0x0a, 0x00, 0x00, 0x00, 0x07, 0x05, 0x02, 0x02, 0x40, 0x00, 0x00, 0x07, 0x05,
//             0x83, 0x02, 0x40, 0x00, 0x00, 0x09, 0x04, 0x02, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00,
//             0x09, 0x21, 0x01, 0x01, 0x00, 0x01, 0x22, 0x35, 0x00, 0x07, 0x05, 0x84, 0x03, 0x40,
//             0x00, 0x01, 0x09, 0x04, 0x03, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00, 0x09, 0x21, 0x01,
//             0x01, 0x00, 0x01, 0x22, 0x72, 0x00, 0x07, 0x05, 0x85, 0x03, 0x40, 0x00, 0x01, 0x09,
//             0x04, 0x04, 0x00, 0x01, 0x03, 0x01, 0x01, 0x00, 0x09, 0x21, 0x01, 0x01, 0x00, 0x01,
//             0x22, 0x3f, 0x00, 0x07, 0x05, 0x86, 0x03, 0x40, 0x00, 0x01,
//         ];
//
//         let (got_inum, got) = ep_for_midi_class(raw).expect("Looking for endpoint");
//         let want = EndpointDescriptor {
//             b_length: 7,
//             b_descriptor_type: DescriptorType::Endpoint,
//             b_endpoint_address: 0x86,
//             bm_attributes: 0x03,
//             w_max_packet_size: 64,
//             b_interval: 1,
//         };
//         assert_eq!(got_inum, 4);
//         assert_eq!(*got, want);
//     }
//
//     struct DummyHost {
//         fail: bool,
//     }
//
//     impl USBHost for DummyHost {
//         fn control_transfer(
//             &mut self,
//             _ep: &mut dyn Endpoint,
//             _bm_request_type: RequestType,
//             _b_request: RequestCode,
//             _w_value: WValue,
//             _w_index: u16,
//             _buf: Option<&mut [u8]>,
//         ) -> Result<usize, TransferError> {
//             if self.fail {
//                 Err(TransferError::Permanent("foo"))
//             } else {
//                 Ok(0)
//             }
//         }
//
//         fn in_transfer(
//             &mut self,
//             _ep: &mut dyn Endpoint,
//             _buf: &mut [u8],
//         ) -> Result<usize, TransferError> {
//             if self.fail {
//                 Err(TransferError::Permanent("foo"))
//             } else {
//                 Ok(0)
//             }
//         }
//
//         fn out_transfer(
//             &mut self,
//             _ep: &mut dyn Endpoint,
//             _buf: &[u8],
//         ) -> Result<usize, TransferError> {
//             if self.fail {
//                 Err(TransferError::Permanent("foo"))
//             } else {
//                 Ok(0)
//             }
//         }
//     }
// }
