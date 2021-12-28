use crate::status::{is_non_status, is_channel_status, SYSEX_END};
use crate::{CodeIndexNumber, Packet, Status, MidiError};
use core::convert::TryFrom;

#[derive(Copy, Clone, Default, Debug)]
struct PacketBuffer {
    expected_len: u8,
    len: u8,
    bytes: [u8; 4],
}


impl PacketBuffer {
    pub fn is_full(&self) -> bool {
        self.len >= self.expected_len
    }

    pub fn is_started(&self) -> bool {
        self.len != 0
    }

    pub fn push(&mut self, byte: u8) {
        assert!(!self.is_full(), "MIDI Packet Length Exceeded {} >= {}", self.len, self.expected_len);
        self.len += 1;
        self.bytes[self.len as usize] = byte;
    }

    pub fn build(&mut self, cin: CodeIndexNumber) -> Packet {
        self.bytes[0] = cin as u8;
        let packet = Packet::from_raw(self.bytes);
        self.clear(self.expected_len);
        packet
    }

    pub fn clear(&mut self, new_limit: u8) {
        self.len = 0;
        self.bytes = [0; 4];
        self.expected_len = new_limit;
    }
}

/// USB Event Packets are used to move MIDI across Serial and USB devices
#[derive(Debug, Default)]
pub struct PacketParser {
    status: Option<Status>,
    buffer: PacketBuffer,
}

impl PacketParser {
    /// Push new payload byte
    /// returns:
    /// - Ok(None) if packet is incomplete
    /// - Ok(Some(packet)) if packet is complete - should not be pushed to anymore, waiting on either sysex or sysex_end
    pub fn advance(&mut self, byte: u8) -> Result<Option<Packet>, MidiError> {
        if is_non_status(byte) {
            if let Some(status) = self.status {
                if !self.buffer.is_started() && is_channel_status(status as u8) {
                    // running status, repeat last
                    self.buffer.clear(self.buffer.expected_len);
                    self.buffer.push(status as u8);
                }
                self.buffer.push(byte);

                if byte == SYSEX_END {
                    self.status = None;
                    return Ok(Some(self.buffer.build(CodeIndexNumber::end_sysex(self.buffer.len)?)));
                }
                if self.buffer.is_full() {
                    return Ok(Some(self.buffer.build(CodeIndexNumber::from(status))));
                }
            }
            return Ok(None);
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
        }
        Ok(None)
    }
}
