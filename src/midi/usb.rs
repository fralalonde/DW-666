use usb_device::device::UsbVidPid;
use usb_device::device::{UsbDevice, UsbDeviceState};
use usb_device::prelude::UsbDeviceBuilder;

use usb_device::{
    bus::{InterfaceNumber, UsbBus, UsbBusAllocator},
    class::UsbClass,
    descriptor::DescriptorWriter,
    endpoint::{EndpointIn, EndpointOut},
    UsbError,
};

use core::result::Result;

use stm32f1xx_hal::usb::{UsbBusType};
use crate::midi::packet::MidiPacket;
use crate::midi::MidiError;
use crate::midi;

const USB_BUFFER_SIZE: u16 = 64;

// const MIDI_IN_SIZE: u8 = 0x06;
const MIDI_OUT_SIZE: u8 = 0x09;

const USB_CLASS_NONE: u8 = 0x00;
const USB_AUDIO_CLASS: u8 = 0x01;
const USB_AUDIOCONTROL_SUBCLASS: u8 = 0x01;
const USB_MIDISTREAMING_SUBCLASS: u8 = 0x03;

const MIDI_IN_JACK_SUBTYPE: u8 = 0x02;
const MIDI_OUT_JACK_SUBTYPE: u8 = 0x03;

const EMBEDDED: u8 = 0x01;
const CS_INTERFACE: u8 = 0x24;
const CS_ENDPOINT: u8 = 0x25;
const HEADER_SUBTYPE: u8 = 0x01;
const MS_HEADER_SUBTYPE: u8 = 0x01;
const MS_GENERAL: u8 = 0x01;

/// Configures the usb device as seen by the operating system.
pub fn configure_usb<B: UsbBus>(usb_bus: &UsbBusAllocator<B>) -> UsbDevice<B> {
    let usb_vid_pid = UsbVidPid(0x16c0, 0x27dd);
    let usb_dev = UsbDeviceBuilder::new(usb_bus, usb_vid_pid)
        .manufacturer("Roto")
        .product("USB MIDI Router")
        .serial_number("123")
        .device_class(USB_CLASS_NONE)
        .build();
    usb_dev
}

pub struct UsbMidi {
    usb_dev: UsbDevice<'static, UsbBusType>,
    midi_class: MidiClass<'static, UsbBusType>,
    buffer: [u8; 64],
    buf_len: usize,
    buf_idx: usize,
}

impl UsbMidi {
    pub fn new(usb_dev: UsbDevice<'static, UsbBusType>,
               midi_class: MidiClass<'static, UsbBusType>,
    ) -> Self {
        UsbMidi {
            midi_class,
            usb_dev,
            buffer: [0; 64],
            buf_len: 0,
            buf_idx: 0,
        }
    }

    pub fn poll(&mut self) -> bool {
        self.usb_dev.poll(&mut [&mut self.midi_class])
    }
}

impl midi::Transmit for UsbMidi {
    fn transmit(&mut self, packet: MidiPacket) -> Result<(), MidiError> {
        if self.usb_dev.state() == UsbDeviceState::Configured {
            self.midi_class.send(packet.raw())?;
        }
        Ok(())
    }
}

const PACKET_LEN: usize = 4;

impl midi::Receive for UsbMidi {
    fn receive(&mut self) -> Result<Option<MidiPacket>, MidiError> {
        if self.buf_idx > self.buf_len {
            if self.usb_dev.state() != UsbDeviceState::Configured {
                return Ok(None);
            }
            match self.midi_class.receive(&mut self.buffer) {
                Ok(Some(size)) if size >= PACKET_LEN => {
                    self.buf_len = size / PACKET_LEN;
                    self.buf_idx = 0;
                }
                Ok(Some(_)) => return Ok(None),
                Ok(None) => return Ok(None),
                Err(UsbError::WouldBlock) => return Ok(None),
                Err(err) => return Err(err.into()),
            }
        }
        let raw = self.buffer.as_chunks().0[self.buf_idx];
        self.buf_idx += 1;
        Ok(Some(MidiPacket::from_raw(raw)?))
    }
}

///Note we are using MidiIn here to refer to the fact that
///The Host sees it as a midi in device
///This class allows you to send types in
pub struct MidiClass<'a, B: UsbBus> {
    standard_ac: InterfaceNumber,
    standard_mc: InterfaceNumber,
    standard_bulkout: EndpointOut<'a, B>,
    standard_bulkin: EndpointIn<'a, B>,
}

impl<B: UsbBus> MidiClass<'_, B> {
    /// Creates a new MidiClass with the provided UsbBus
    pub fn new(usb_alloc: &UsbBusAllocator<B>) -> MidiClass<'_, B> {
        MidiClass {
            standard_ac: usb_alloc.interface(),
            standard_mc: usb_alloc.interface(),
            standard_bulkout: usb_alloc.bulk(USB_BUFFER_SIZE),
            standard_bulkin: usb_alloc.bulk(USB_BUFFER_SIZE),
        }
    }

    /// Return the number of sent bytes
    pub fn send(&mut self, payload: &[u8]) -> Result<usize, usb_device::UsbError> {
        self.standard_bulkin.write(payload)
    }

    /// Return the number of received payload bytes (possibly zero)
    /// Returns None if no data was available
    pub fn receive(&mut self, payload: &mut [u8]) -> Result<Option<usize>, usb_device::UsbError> {
        match self.standard_bulkout.read(payload) {
            Ok(size) => Ok(Some(size)),
            Err(UsbError::WouldBlock) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl<B: UsbBus> UsbClass<B> for MidiClass<'_, B> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<(), usb_device::UsbError> {
        writer.interface(
            self.standard_ac,
            USB_AUDIO_CLASS,
            USB_AUDIOCONTROL_SUBCLASS,
            0x00, // no protocol
        )?;

        writer.write(CS_INTERFACE, &[
            HEADER_SUBTYPE,
            0x00,
            0x01, // Revision
            0x09,
            0x00, // SIZE of class specific descriptions
            0x01, // Number of streaming interfaces
            0x01, // MIDIStreaming interface 1 belongs to this AC interface
        ])?;

        // Streaming Standard
        writer.interface(
            self.standard_mc,
            USB_AUDIO_CLASS,
            USB_MIDISTREAMING_SUBCLASS,
            0,
        )?;

        // Streaming Extras
        writer.write(CS_INTERFACE, &[
            MS_HEADER_SUBTYPE,
            0x00,
            0x01, // Revision
            0x07 + MIDI_OUT_SIZE,
            0x00,
        ])?;

        // Jacks
        writer.write(CS_INTERFACE, &[MIDI_IN_JACK_SUBTYPE, EMBEDDED, 0x01, 0x00])?;

        writer.write(CS_INTERFACE, &[
            MIDI_OUT_JACK_SUBTYPE,
            EMBEDDED,
            0x01,
            0x01,
            0x01,
            0x01,
            0x00,
        ])?;

        writer.endpoint(&self.standard_bulkout)?;
        writer.write(CS_ENDPOINT, &[MS_GENERAL, 0x01, 0x01])?;

        writer.endpoint(&self.standard_bulkin)?;
        writer.write(CS_ENDPOINT, &[MS_GENERAL, 0x01, 0x01])?;
        Ok(())
    }
}
