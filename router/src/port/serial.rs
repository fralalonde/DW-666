//! MIDI using HAL Serial

use core::convert::TryFrom;
use embedded_hal::serial::{Write, Read};

use hal::serial::{Event, Pins, Instance};

use heapless::spsc::Queue;
use midi::{Packet, MidiError, CableNumber, Receive, Transmit, PacketList};

pub struct SerialMidi<UART, PINS> {
    pub uart: hal::serial::Serial<UART, PINS>,
    pub tx_fifo: Queue<u8, 64>,
    cable_number: CableNumber,
    parser: midi::PacketParser,
    last_status: Option<u8>,
}

impl<UART, PINS> SerialMidi<UART, PINS> where
    PINS: Pins<UART>,
    UART: Instance,
{
    pub fn new(uart: hal::serial::Serial<UART, PINS>, cable_number: CableNumber) -> Self {
        SerialMidi {
            uart,
            tx_fifo: Queue::new(),
            cable_number,
            parser: midi::PacketParser::default(),
            last_status: None,
        }
    }

    pub fn flush(&mut self) -> Result<(), MidiError> {
        'write_bytes:
        loop {
            if self.uart.is_txe() {
                if let Some(byte) = self.tx_fifo.dequeue() {
                    self.uart.write(byte)?;
                    continue 'write_bytes;
                } else {
                    self.uart.unlisten(Event::Txe)
                }
            } else {
                self.uart.listen(Event::Txe)
            }
            return Ok(());
        }
    }

    fn write_all(&mut self, payload: &[u8]) -> Result<(), MidiError> {
        for byte in payload {
            self.write_byte(*byte)?
        }
        Ok(())
    }

    fn write_byte(&mut self, byte: u8) -> Result<(), MidiError> {
        self.tx_fifo.enqueue(byte).map_err(|_| MidiError::BufferFull)?;
        Ok(())
    }
}

impl<UART, PINS> Receive for SerialMidi<UART, PINS> where
    PINS: Pins<UART>,
    UART: Instance,
{
    fn receive(&mut self) -> Result<Option<Packet>, MidiError> {
        if self.uart.is_rxne() {
            let byte = self.uart.read()?;
            let packet = self.parser.advance(byte)?;
            if let Some(packet) = packet {
                return Ok(Some(packet.with_cable_num(self.cable_number)));
            }
        }
        Ok(None)
    }
}

impl<UART, PINS> Transmit for SerialMidi<UART, PINS> where
    PINS: Pins<UART>,
    UART: Instance,
{
    fn transmit(&mut self, packets: PacketList) -> Result<(), MidiError> {
        for packet in packets.iter() {
            let mut payload = packet.payload();
            // Apply MIDI "running status"
            if midi::is_channel_status(payload[0]) {
                if let Some(last_status) = self.last_status {
                    if payload[0] == last_status {
                        // same status as last time, chop out status byte
                        payload = &payload[1..];
                    } else {
                        // take note of new status
                        self.last_status = Some(payload[0])
                    }
                }
            } else {
                // non-repeatable status or no status (sysex)
                self.last_status = None
            }
            self.write_all(payload)?;
        }
        self.flush()?;
        Ok(())
    }
}


