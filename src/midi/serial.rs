//! *Midi driver on top of embedded hal serial communications*
//!
use crate::midi::status::{SYSEX_END, is_non_status, is_channel_status, SYSEX_START};
use embedded_hal::serial;
use crate::midi::packet::{Packet, CableNumber, CodeIndexNumber};
use crate::midi::{MidiError, Receive, Transmit};
use crate::midi::status::Status;
use core::convert::TryFrom;
use embedded_hal::serial::Write;

#[cfg(feature = "stm32f4xx")]
use stm32f4xx_hal as hal;
use hal::{
    serial::{config::Config, Event, Serial, Rx, Tx, config::StopBits},
    stm32::USART2,
};

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

pub struct SerialIn<RX> {
    serial_in: RX,
    cable_number: CableNumber,
    parser: PacketParser,
}

impl<RX, E> SerialIn<RX>
    where RX: serial::Read<u8, Error=E>,
{
    pub fn new(rx: RX, cable_number: CableNumber) -> Self {
        SerialIn {
            serial_in: rx,
            cable_number,
            parser: PacketParser::default(),
        }
    }
}

impl<RX, E> Receive for SerialIn<RX>
    where RX: serial::Read<u8, Error=E>
{
    fn receive(&mut self) -> Result<Option<Packet>, MidiError> {
        let byte = self.serial_in.read()?;
        let packet = self.parser.advance(byte);
        if let Ok(Some(packet)) = packet {
            return Ok(Some(packet.with_cable_num(self.cable_number)));
        }
        packet
    }
}

const TX_FIFO_SIZE: u8 = 128;


// FIXME USART should be a type parameter but this makes Tx::listen and Tx::unlisten (used in flush()) inaccessible. why?
// TODO might try using DMA instead
pub struct SerialOut/*<USART>*/ {
    serial_out: Tx<USART2>,
    last_status: Option<u8>,

    tx_fifo: [u8; TX_FIFO_SIZE as usize],
    tx_head: u8,
    tx_tail: u8,
}

impl/*<USART>*/ SerialOut/*<USART>*/
// where Tx<USART>: serial::Write<u8>
{
    pub fn new(tx: Tx<USART2>) -> Self {
        SerialOut {
            serial_out: tx,
            last_status: None,
            // serial tx uses a circular buffer
            tx_fifo: [0; TX_FIFO_SIZE as usize],
            tx_head: 0,
            tx_tail: 0,
        }
    }

    fn buf_len(&self) -> u8 {
        if self.tx_head > self.tx_tail {
            self.tx_head - self.tx_tail
        } else {
            self.tx_head + (TX_FIFO_SIZE - self.tx_tail)
        }
    }

    pub fn flush(&mut self) -> Result<(), MidiError> {
        while self.buf_len() > 0 {
            match self.serial_out.write(self.tx_fifo[self.tx_tail as usize]) {
                Err(nb::Error::WouldBlock) => {
                    self.serial_out.listen();
                    return Ok(())
                }
                Err(_err) => {
                    rprintln!("Failed to write serial payload for reason other than blocking");
                    return Err(MidiError::SerialError);
                }
                Ok(_) => {
                    if self.tx_tail == (TX_FIFO_SIZE - 1) {
                        self.tx_tail = 0
                    } else {
                        self.tx_tail += 1
                    }
                }
            }
        }
        self.serial_out.unlisten();
        Ok(())
    }

    fn write_all(&mut self, payload: &[u8]) -> Result<(), MidiError> {
        for byte in payload {
            self.write_byte(*byte)?
        }
        Ok(())
    }

    fn write_byte(&mut self, byte: u8) -> Result<(), MidiError> {
        if self.buf_len() >= TX_FIFO_SIZE {
            return Err(MidiError::BufferFull)
        }
        self.tx_fifo[self.tx_head as usize] = byte;
        if self.tx_head == (TX_FIFO_SIZE - 1) {
            self.tx_head = 0
        } else {
            self.tx_head += 1
        }
        Ok(())
    }
}

impl/*<USART>*/ Transmit for SerialOut/*<USART>*/
    // where Tx<USART>: serial::Write<u8>
{
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

        self.write_all(payload);
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

