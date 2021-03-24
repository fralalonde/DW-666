use crate::midi::{Packet, CodeIndexNumber, MidiError};
use crate::midi::status::{is_non_status, SYSEX_END, SYSEX_START};
use crate::midi::packet::CodeIndexNumber::SystemCommonLen1;
use CodeIndexNumber::{SysexEndsNext2, SysexEndsNext3, Sysex};
use heapless::Vec;

struct SysexCapture<const N: usize> {
    inner: [u8; N]
}

impl<const N: usize> SysexCapture<N> {

    pub fn new() -> Self {
        SysexCapture {
            inner: [0; N]
        }
    }

    /// Returns the number of bytes added
    pub fn copy_sysex_from_body(&mut self, packet: Packet) -> Result<u8, MidiError> {
        let body = packet.sysex_body();
        if self.inner.len() < body.len() {
            Err(MidiError::SysexBufferFull)?
        }
        self.inner[..body.len()].copy_from_slice(body);
        Ok(body.len() as u8)
    }
}
