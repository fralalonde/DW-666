//! *Midi driver on top of embedded hal serial communications*
//!
use crate::midi::status::{SYSEX_END, is_non_status, is_channel_status, SYSEX_START};
use embedded_hal::serial;
use crate::midi::packet::{Packet, CableNumber, CodeIndexNumber};
use crate::midi::{MidiError, Receive, Transmit};
use crate::midi::status::Status;
use core::convert::TryFrom;

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


pub struct SerialOut<TX> {
    serial_out: TX,
    last_status: Option<u8>,
}

impl<TX> SerialOut<TX>
    where TX: serial::Write<u8>
{
    pub fn new(tx: TX) -> Self {
        SerialOut {
            serial_out: tx,
            last_status: None,
        }
    }

    fn write_all(&mut self, payload: &[u8]) -> Result<(), MidiError> {
        for byte in payload {
            self.write_byte(*byte)?
        }
        Ok(())
    }

    fn write_byte(&mut self, byte: u8) -> Result<(), MidiError> {
        // TODO try using TXE interrupt callback instead
        let mut tries = 0;
        loop {
            match self.serial_out.write(byte) {
                Err(nb::Error::WouldBlock) => {
                    tries += 1;
                    if tries > 10000 {
                        rprintln!("Write failed, Serial port _still_ in use after many retries");
                        return Err(MidiError::SerialError);
                    }
                }
                Err(_err) => {
                    rprintln!("Failed to write serial payload for reason other than blocking");
                    return Err(MidiError::SerialError);
                }
                _ => return Ok(())
            }
        }
    }
}

impl<TX> Transmit for SerialOut<TX>
    where TX: serial::Write<u8>
{
    fn transmit(&mut self, event: Packet) -> Result<(), MidiError> {
        let mut payload = event.payload();

        // Apply MIDI "running status" optimization
        match event.code_index_number() {
            // FIXME full optimization would also include Sysex? (except Realtime class) - whatever
            CodeIndexNumber::Sysex
            | CodeIndexNumber::SysexEndsNext2
            | CodeIndexNumber::SysexEndsNext3 => {}
            _ => {
                let new_status = Some(payload[0]);
                if self.last_status == new_status {
                    payload = &payload[1..];
                } else {
                    self.last_status = new_status;
                }
            }
        }
        self.write_all(payload);
        Ok(())
    }

    fn transmit_sysex(&mut self, payload: &[u8]) -> Result<(), MidiError> {
        self.write_byte(SYSEX_START)?;
        self.write_all(payload)?;
        self.write_byte(SYSEX_END)?;
        Ok(())
    }
}
