//! *Midi driver on top of embedded hal serial communications*
//!
use crate::midi::status::{SYSEX_END, is_non_status, is_channel_status, SYSEX_START};
use embedded_hal::serial;
use crate::midi::packet::{Packet, CableNumber, CodeIndexNumber};
use crate::midi::{MidiError, Receive, Transmit};
use crate::midi::status::Status;
use core::convert::TryFrom;
use embedded_hal::serial::{Write, Read};

const TX_FIFO_SIZE: u8 = 128;

use stm32f4xx_hal as hal;
use hal::{
    serial::{config::Config, Event, Serial, Rx, Tx, config::StopBits},
    stm32::USART2,
};
use hal::gpio::AF7;
use hal::gpio::gpioa::{PA2, PA3};
use hal::serial::Event::Txe;

#[derive(Copy, Clone, Default, Debug)]
struct PacketBuffer {
    expected_len: u8,
    len: u8,
    bytes: [u8; 4],
}

impl PacketBuffer {
    fn is_full(&self) -> bool {
        self.len >= self.expected_len
    }

    fn is_started(&self) -> bool {
        self.len != 0
    }

    fn push(&mut self, byte: u8) {
        assert!(!self.is_full(), "MIDI Packet Length Exceeded {} >= {}", self.len, self.expected_len);
        self.len += 1;
        self.bytes[self.len as usize] = byte;
    }

    fn build(&mut self, cin: CodeIndexNumber) -> Option<Packet> {
        self.bytes[0] = cin as u8;
        let packet = Packet::from_raw(self.bytes);
        self.clear(self.expected_len);
        Some(packet)
    }

    fn clear(&mut self, new_limit: u8) {
        self.len = 0;
        self.bytes = [0; 4];
        self.expected_len = new_limit;
    }
}

/// USB Event Packets are used to move MIDI across Serial and USB devices
#[derive(Debug, Default)]
struct PacketParser {
    status: Option<Status>,
    buffer: PacketBuffer,
}

impl PacketParser {
    /// Push new payload byte
    /// returns:
    /// - Ok(None) if packet is incomplete
    /// - Ok(Some(packet)) if packet is complete - should not be pushed to anymore, waiting on either sysex or sysex_end
    /// - MidiError::ParseCritical if parser failed to ingest with no chance of retry
    /// - MidiError::ParseCritical if parser failed to ingest with no chance of retry
    fn advance(&mut self, byte: u8) -> Result<Option<Packet>, MidiError> {
        if is_non_status(byte) {
            if let Some(status) = self.status {
                if !self.buffer.is_started() && is_channel_status(status as u8) {
                    // running status, repeat last
                    self.buffer.clear(self.buffer.expected_len);
                    self.buffer.push(status as u8);
                }

                self.buffer.push(byte);

                return Ok(if self.buffer.is_full() {
                    if byte == SYSEX_END {
                        self.status = None;
                        self.buffer.build(CodeIndexNumber::end_sysex(self.buffer.len)?)
                    } else {
                        self.buffer.build(CodeIndexNumber::from(status))
                    }
                } else {
                    None
                });
            }
        }

        if let Ok(status) = Status::try_from(byte) {
            match status.expected_len() {
                1 => {
                    // single-byte message do not need running status
                    self.status = None;

                    // skip buffer for single-byte messages
                    return Ok(Some(Packet::from_raw([CodeIndexNumber::from(status) as u8, byte, 0, 0])));
                }
                expected_len => {
                    self.status = Some(status);
                    self.buffer.clear(expected_len);
                    self.buffer.push(byte);
                }
            }
        } else {
            rprintln!("status parse error");
        }
        Ok(None)
    }
}

pub type UartPeripheral = hal::serial::Serial<
    USART2,
    (
        PA2<hal::gpio::Alternate<AF7>>,
        PA3<hal::gpio::Alternate<AF7>>,
    ),
>;

pub type Queue = heapless::spsc::Queue<u8, u8, 32>;

pub struct SerialMidi {
    pub uart: UartPeripheral,
    pub tx_fifo: Queue,
    cable_number: CableNumber,
    parser: PacketParser,
    last_status: Option<u8>,
}

impl SerialMidi {
    pub fn new(handle: UartPeripheral, cable_number: CableNumber) -> Self {
        SerialMidi {
            uart: handle,
            tx_fifo: unsafe { Queue::u8() },
            cable_number,
            parser: PacketParser::default(),
            last_status: None,
        }
    }

    pub fn flush(&mut self) -> Result<(), MidiError> {
        if self.uart.is_txe() {
            loop {
                if let Some(byte) = self.tx_fifo.dequeue() {
                    match self.uart.write(byte) {
                        Err(nb::Error::WouldBlock) => {
                            self.uart.listen(Txe);
                            return Ok(());
                        }
                        Err(_err) => {
                            rprintln!("Failed to write serial payload for reason other than blocking");
                            return Err(MidiError::SerialError);
                        }
                        Ok(_) => {}
                    }
                } else {
                    self.uart.unlisten(Txe);
                    break;
                }
            }
        }
        Ok(())
    }

    fn write_all(&mut self, payload: &[u8]) -> Result<(), MidiError> {
        for byte in payload {
            self.write_byte(*byte)?
        }
        Ok(())
    }

    fn write_byte(&mut self, byte: u8) -> Result<(), MidiError> {
        self.tx_fifo.enqueue(byte).map_err(|e| MidiError::BufferFull)

    }
}

impl Receive for SerialMidi {
    fn receive(&mut self) -> Result<Option<Packet>, MidiError> {
        if self.uart.is_rxne() {
            let byte = self.uart.read()?;
            let packet = self.parser.advance(byte);
            if let Ok(Some(packet)) = packet {
                return Ok(Some(packet.with_cable_num(self.cable_number)));
            }
            packet
        } else {
            Ok(None)
        }
    }
}

impl Transmit for SerialMidi {
    fn transmit(&mut self, event: Packet) -> Result<(), MidiError> {
        let mut payload = event.payload();

        // Apply MIDI "running status"
        if is_channel_status(payload[0]) {
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
        self.flush()?;
        Ok(())
    }

    fn transmit_sysex(&mut self, payload: &[u8]) -> Result<(), MidiError> {
        self.write_byte(SYSEX_START)?;
        self.write_all(payload)?;
        self.write_byte(SYSEX_END)?;
        self.flush()?;
        Ok(())
    }
}


