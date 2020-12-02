//! *Midi driver on top of embedded hal serial communications*
//!
use crate::midi::message::{MidiStatus};
use core::fmt::Debug;
use embedded_hal::serial;
use usb_device::UsbError::WouldBlock;
use crate::midi::event::{Packet, CableNumber, CodeIndexNumber, PacketBuilder};
use alloc::vec::Vec;
use alloc::collections::VecDeque;

pub struct MidiIn<RX> {
    serial_in: RX,
    cable_number: CableNumber,
    builder: PacketBuilder,
}

impl<RX, E> MidiIn<RX>
where
    RX: serial::Read<u8, Error = E>,
    E: Debug,
{
    pub fn new(rx: RX, cable_number: CableNumber) -> Self {
        MidiIn {
            serial_in: rx,
            cable_number,
            builder: PacketBuilder::new(),
        }
    }

    pub fn read(&mut self) -> Result<Option<Packet>, E> {
        let byte = self.serial_in.read()?;
        let mut packet = self.builder.advance(byte);
        if let Err(err) = packet {
            // TODO record error
            // reset builder & retry with same byte
            self.builder = PacketBuilder::new();
            packet = self.builder.advance(byte);
        }
        match self.builder.advance(byte) {
            // retry failed
            Err(err) => Err(err),

            Ok((builder, packet)) => {
                self.builder = builder;
                Ok(packet)
            }
        }
    }
}

pub struct MidiOut<TX> {
    serial_out: TX,
    last_status: Option<u8>,
}

impl<TX, E> MidiOut<TX>
where
    TX: serial::Write<u8, Error = E>,
    E: Debug,
{
    pub fn new(tx: TX) -> Self {
        MidiOut {
            serial_out: tx,
            last_status: None,
        }
    }

    pub fn release(self) -> TX {
        self.serial_out
    }

    fn send(&mut self, event: Packet) -> Result<(), E> {
        let mut payload = event.payload();
        let new_status = Some(payload[0]);
        if self.last_status == new_status {
            payload = &payload[1..];
        } else {
            self.last_status = new_status;
        }

        for byte in payload {
            self.serial_out.write(*byte)?;
        }

        Ok(())
    }
}
