//! *Midi driver on top of embedded hal serial communications*
//!
use core::fmt::Debug;
use embedded_hal::serial;
pub use crate::midi::parser::Parser;
use usb_device::UsbError::WouldBlock;
use crate::midi::message::MidiFragment;

pub struct MidiIn<RX> {
    serial_in: RX,
    parser: Parser,
}

impl<RX, E> MidiIn<RX>
    where
        RX: serial::Read<u8, Error = E>,
        E: Debug,
{
    pub fn new(rx: RX) -> Self {
        MidiIn {
            serial_in: rx,
            parser: Parser::new(),
        }
    }

    pub fn read(&mut self) -> Result<MidiMessage, E> {
        let byte = self.serial_in.read()?;

        match self.parser.advance(byte) {
            Some(event) => Ok(event),
            None => Err(WouldBlock),
        }
    }
}

pub struct MidiOut<TX> {
    serial_out: TX,
    last_status: Option<u8>,
}

impl<TX, E> MidiOut<TX>
    where TX: serial::Write<u8, Error = E>,
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

    fn send(&mut self, fragment: MidiFragment) -> Result<(), E> {
        let status = status_msb + channel;
        // If the last command written had the same status/channel, the MIDI protocol allows us to
        // omit sending the status byte again.
        if self.last_status != Some(status) {
            self.serial_out.write(status)?;
        }
        for byte in data {
            self.serial_out.write(*byte)?;
        }
        self.last_status = Some(status);

        Ok(())
    }
}

