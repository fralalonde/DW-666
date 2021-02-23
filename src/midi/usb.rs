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

use rtt_target::rprintln;

use core::result::Result;

use stm32f1xx_hal::usb::{UsbBusType};
use crate::midi::packet::MidiPacket;
use crate::midi::MidiError;
use crate::midi;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::Relaxed;
use usb_device::class_prelude::EndpointAddress;

const USB_TX_BUFFER_SIZE: u16 = 256;
const USB_RX_BUFFER_SIZE: u16 = 64;

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
        .manufacturer("M'Roto")
        .product("USB MIDI Router")
        .serial_number("123")
        .device_class(USB_CLASS_NONE)
        .build();
    usb_dev
}

const PACKET_LEN: usize = 4;
const TX_BUF_LEN: usize = USB_TX_BUFFER_SIZE as usize;
const RX_BUF_LEN: usize = USB_RX_BUFFER_SIZE as usize + PACKET_LEN;

pub struct UsbMidi {
    usb_dev: UsbDevice<'static, UsbBusType>,
    midi_class: MidiClass<'static, UsbBusType>,

    // tx_buf: [u8; TX_BUF_LEN],
    // tx_end: usize,
    // tx_start: usize,

    rx_buf: [u8; RX_BUF_LEN],
    rx_end: usize,
    rx_start: usize,
}

impl UsbMidi {
    pub fn new(usb_dev: UsbDevice<'static, UsbBusType>,
               midi_class: MidiClass<'static, UsbBusType>,
    ) -> Self {
        UsbMidi {
            midi_class,
            usb_dev,
            // tx_buf: [0; TX_BUF_LEN],
            // tx_start: 0,
            // tx_end: 0,
            rx_buf: [0; RX_BUF_LEN],
            rx_start: 0,
            rx_end: 0,
        }
    }

    /// USB upkeep
    pub fn poll(&mut self) -> bool {
        self.usb_dev.poll(&mut [&mut self.midi_class])
    }

    // #[inline]
    // fn tx_size(&self) -> usize {
    //     self.tx_end - self.tx_start
    // }
    //
    // fn tx_push(&mut self) -> Result<Option<MidiPacket>, MidiError> {
    //     if self.tx_size() >= PACKET_LEN {
    //         let raw = MidiPacket::from_raw(self.tx_buf.as_chunks().0[0])?;
    //         self.tx_start += PACKET_LEN;
    //         Ok(Some(raw))
    //     } else {
    //         Ok(None)
    //     }
    // }
    //
    // fn tx_compact(&mut self) {
    //     self.tx_buf.copy_within(self.tx_start..self.tx_end, 0);
    //     self.tx_end = self.tx_size();
    //     self.tx_start = 0;
    // }
    //
    // fn tx_flush(&mut self) -> Result<(), UsbError> {
    //     let result = self.midi_class.receive(&mut self.tx_buf[self.tx_end..tx_BUF_LEN]);
    //     match result {
    //         Ok(received) => {
    //             self.tx_end += received;
    //             assert!(self.tx_end <= self.tx_buf.len());
    //             Ok(())
    //         },
    //         Err(UsbError::WouldBlock) => Ok(()),
    //         Err(err) => panic!("{:?}", err),
    //     }
    // }

    #[inline]
    fn rx_size(&self) -> usize {
        self.rx_end - self.rx_start
    }
    
    fn rx_pop(&mut self) -> Result<Option<MidiPacket>, MidiError> {
        if self.rx_size() >= PACKET_LEN {
            let raw = MidiPacket::from_raw(self.rx_buf.as_chunks().0[0])?;
            self.rx_start += PACKET_LEN;
            Ok(Some(raw))
        } else {
            Ok(None)
        }
    }
    
    fn rx_compact(&mut self) {
        self.rx_buf.copy_within(self.rx_start..self.rx_end, 0);
        self.rx_end = self.rx_size();
        self.rx_start = 0;
    }

    fn rx_fill(&mut self) -> Result<(), UsbError> {
        let result = self.midi_class.receive(&mut self.rx_buf[self.rx_end..RX_BUF_LEN]);
        match result {
            Ok(received) => {
                self.rx_end += received;
                assert!(self.rx_end <= self.rx_buf.len());
                Ok(())
            },
            Err(UsbError::WouldBlock) => Ok(()),
            Err(err) => panic!("{:?}", err),
        }
    }
}

impl midi::Transmit for UsbMidi {
    fn transmit(&mut self, packet: MidiPacket) {
        if self.usb_dev.state() == UsbDeviceState::Configured {
            match self.midi_class.send(packet.raw()) {
                Err(UsbError::WouldBlock) => rprintln!("ERROR: USB TX packet dropped"),
                Err(err) => panic!("{:?}", err),
                Ok(_sent) => {}
            }
        }
    }
}


impl midi::Receive for UsbMidi {
    fn receive(&mut self) -> Result<Option<MidiPacket>, MidiError> {
        if let Some(packet) = self.rx_pop()? {
            Ok(Some(packet))
        } else {
            self.rx_compact();
            self.rx_fill();
            Ok(self.rx_pop()?)
        }
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
            standard_bulkout: usb_alloc.bulk(USB_TX_BUFFER_SIZE),
            standard_bulkin: usb_alloc.bulk(USB_RX_BUFFER_SIZE),
        }
    }

    /// Return the number of sent bytes
    pub fn send(&mut self, payload: &[u8]) -> Result<usize, usb_device::UsbError> {
        self.standard_bulkin.write(payload)
    }

    /// Return the number of received bytes
    pub fn receive(&mut self, payload: &mut [u8]) -> Result<usize, usb_device::UsbError> {
        self.standard_bulkout.read(payload)
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

    /// Called when endpoint with address `addr` has completed transmitting data (IN packet).
    ///
    /// Note: This method may be called for an endpoint address you didn't allocate, and in that
    /// case you should ignore the event.
    fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
        let _ = addr;
    }
}
