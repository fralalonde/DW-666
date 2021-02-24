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
use alloc::boxed::Box;

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

}

impl UsbMidi {
    pub fn new(usb_dev: UsbDevice<'static, UsbBusType>,
               midi_class: MidiClass<'static, UsbBusType>,
    ) -> Self {
        UsbMidi {
            midi_class,
            usb_dev,
        }
    }

    /// USB upkeep
    pub fn poll(&mut self) -> bool {
        self.usb_dev.poll(&mut [&mut self.midi_class])
    }
}

impl midi::Transmit for UsbMidi {
    fn transmit(&mut self, packet: MidiPacket) -> Result<(), MidiError> {
        Ok(self.midi_class.send(packet.bytes()))
    }
}

impl midi::Receive for UsbMidi {
    fn receive(&mut self) -> Result<Option<MidiPacket>, MidiError> {
        if let Some(bytes) = self.midi_class.receive() {
            return Ok(Some(MidiPacket::from_raw(bytes)?));
        }
        Ok(None)
    }
}

///Note we are using MidiIn here to refer to the fact that
///The Host sees it as a midi in device
///This class allows you to send types in
pub struct MidiClass<'a, B: UsbBus> {
    standard_ac: InterfaceNumber,
    standard_mc: InterfaceNumber,

    bulk_out: EndpointOut<'a, B>,
    bulk_in: EndpointIn<'a, B>,

    tx_buf: [u8; TX_BUF_LEN],
    tx_end: usize,

    rx_buf: [u8; RX_BUF_LEN],
    rx_end: usize,
    rx_start: usize,
}

impl<B: UsbBus> MidiClass<'_, B> {
    /// Creates a new MidiClass with the provided UsbBus
    pub fn new(usb_alloc: &UsbBusAllocator<B>) -> MidiClass<'_, B> {
        MidiClass {
            standard_ac: usb_alloc.interface(),
            standard_mc: usb_alloc.interface(),
            bulk_out: usb_alloc.bulk(USB_TX_BUFFER_SIZE),
            bulk_in: usb_alloc.bulk(USB_RX_BUFFER_SIZE),

            tx_buf: [0; TX_BUF_LEN],
            tx_end: 0,

            rx_buf: [0; RX_BUF_LEN],
            rx_start: 0,
            rx_end: 0,
        }
    }

    /// Try enqueue packet, then flush.
    /// If enqueue failed (because buffer full), retry after flush.
    /// Drop packet if all else fails.
    fn send(&mut self, payload: &[u8]) {
        let pushed = self.tx_push(payload);

        let result = self.bulk_in.write(&self.tx_buf[0..self.tx_end]);
        self.tx_end = 0;

        match result {
            Ok(count) if count > 4 =>
                rprintln!("sent more than 4 bytes"),
            Ok(_count) if !pushed && !self.tx_push(payload) =>
                rprintln!("ERROR: USB TX packet dropped after flush (how?)"),
            Ok(_count)  => {}
            Err(UsbError::WouldBlock) => if !pushed {
                rprintln!("ERROR: USB TX packet dropped after flush bounced")
            }
            Err(err) => panic!("{:?}", err),
        }
    }

    fn tx_push(&mut self, payload: &[u8]) -> bool {
        if self.tx_end < (TX_BUF_LEN - payload.len()) {
            self.tx_buf[self.tx_end..self.tx_end + payload.len()].copy_from_slice(payload);
            self.tx_end += payload.len();
            true
        } else {
            false
        }
    }

    /// Look for buffered bytes
    /// If none, try to get more
    fn receive(&mut self) -> Option<[u8; 4]> {
        if let Some(bytes) = self.rx_pop() {
            Some(bytes)
        } else {
            self.rx_fill();
            self.rx_pop()
        }
    }

    #[inline]
    fn rx_size(&self) -> usize {
        self.rx_end - self.rx_start
    }

    fn rx_pop(&mut self) -> Option<[u8; 4]> {
        if self.rx_size() >= PACKET_LEN {
            let raw = self.rx_buf.as_chunks().0[0];
            self.rx_start += PACKET_LEN;
            Some(raw)
        } else {
            None
        }
    }

    fn rx_fill(&mut self) {
        // compact any odd bytes to buffer start
        self.rx_buf.copy_within(self.rx_start..self.rx_end, 0);
        self.rx_end = self.rx_size();
        self.rx_start = 0;

        match self.bulk_out.read(&mut self.rx_buf[self.rx_end..RX_BUF_LEN]) {
            Ok(received) => {
                self.rx_end += received;
                assert!(self.rx_end <= self.rx_buf.len());
            }
            Err(UsbError::WouldBlock) => {}
            Err(err) => panic!("{:?}", err)
        };
    }
}

impl<B: UsbBus> UsbClass<B> for MidiClass<'_, B> {
    // TODO maybe for sysex...
    // /// Called when endpoint with address `addr` has completed transmitting data (IN packet).
    // fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
    //     if addr == self.bulk_in.address() {
    //         // send any pending bytes in tx_buf ? (maybe for sysex...)
    //         if self.tx_end > 0 {
    //             let result = self.bulk_in.write(&self.tx_buf[0..self.tx_end]);
    //             self.tx_end = 0;
    //
    //             match result {
    //                 Ok(count) => rprintln!("sent {:?} fast followers", count),
    //                 Err(UsbError::WouldBlock) => rprintln!("fast follower would block"),
    //                 Err(err) => panic!("{:?}", err),
    //             }
    //         }
    //     }
    // }

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

        writer.endpoint(&self.bulk_out)?;
        writer.write(CS_ENDPOINT, &[MS_GENERAL, 0x01, 0x01])?;

        writer.endpoint(&self.bulk_in)?;
        writer.write(CS_ENDPOINT, &[MS_GENERAL, 0x01, 0x01])?;
        Ok(())
    }
}
