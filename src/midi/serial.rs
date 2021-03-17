//! *Midi driver on top of embedded hal serial communications*
//!
use crate::midi::status::{SYSEX_START, SYSEX_END, is_midi_status};
use embedded_hal::serial;
use crate::midi::packet::{MidiPacket, CableNumber, CodeIndexNumber};
use crate::midi::{MidiError, Receive, Transmit};
use crate::midi::status::MidiStatus;
use crate::midi::status::SystemStatus::SysexStart;
use core::ops::{Deref, DerefMut};
use core::convert::TryFrom;

#[derive(Copy, Clone, Default)]
pub struct PartialPacket {
    len: usize,
    bytes: [u8; 4]
}

impl PartialPacket {
    /// First byte temporarily used as payload length marker, set to proper Cable (MSB) + Code Index Number (LSB) upon build
    pub fn payload_len(&self) -> usize {
        self.len
    }

    pub fn cmd_len(&self) -> Result<Option<usize>, MidiError> {
        Ok(MidiStatus::try_from(self.bytes[1])?.cmd_len())
    }

    pub fn push(&mut self, byte: u8) {
        self.len += 1;
        if self.len > 3 {
            panic!("Pushed serial byte beyond packet length")
        }
        self.bytes[self.len] = byte
    }

    pub fn build(mut self, cable_number: CableNumber) -> Result<MidiPacket, MidiError> {
        let status = MidiStatus::try_from(self.bytes[1])?;
        self.bytes[0] = CodeIndexNumber::from(status) as u8 | u8::from(cable_number) << 4;
        Ok(MidiPacket::from_raw(self.bytes))
    }

    pub fn end_sysex(mut self, cable_number: CableNumber) -> Result<MidiPacket, MidiError> {
        self.bytes[0] = CodeIndexNumber::end_sysex(self.payload_len())? as u8 | u8::from(cable_number) << 4;
        Ok(MidiPacket::from_raw(self.bytes))
    }
}

/// USB Event Packets are used to move MIDI across Serial and USB devices
pub struct PacketBuilder {
    prev_status: Option<u8>,
    pending_sysex: Option<PartialPacket>,
    inner: PartialPacket,
}

impl Deref for PacketBuilder {
    type Target = PartialPacket;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for PacketBuilder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}


impl PacketBuilder {
    pub fn new() -> Self {
        PacketBuilder { prev_status: None, pending_sysex: None, inner: PartialPacket::default() }
    }

    pub fn next(prev_status: Option<u8>, pending_sysex: Option<PartialPacket>) -> Self {
        PacketBuilder { prev_status, pending_sysex, inner: PartialPacket::default() }
    }

}

pub struct SerialMidiIn<RX> {
    serial_in: RX,
    cable_number: CableNumber,
    builder: PacketBuilder,
}

impl<RX, E> SerialMidiIn<RX>
    where RX: serial::Read<u8, Error=E>,
{
    pub fn new(rx: RX, cable_number: CableNumber) -> Self {
        SerialMidiIn {
            serial_in: rx,
            cable_number,
            builder: PacketBuilder::new(),
        }
    }

    /// Push new command payload byt
    /// returns:
    /// - Ok(false) if packet is incomplete
    /// - Ok(true) if packet is complete - should not be pushed to anymore, waiting on either sysex or sysex_end
    /// - MidiError(PacketOverflow) if packet was complete
    fn advance(&mut self, byte: u8) -> Result<Option<MidiPacket>, MidiError> {
        match (self.builder.payload_len(), byte, self.builder.pending_sysex, self.builder.prev_status) {
            (0, SYSEX_END, Some(pending), _) => {
                // end of sysex stream, release previous packet as final
                self.builder.pending_sysex = None;
                Ok(Some(pending.end_sysex(self.cable_number)?))
            }
            (_, SYSEX_END, None, Some(status)) if status == SysexStart as u8 => {
                // ignore sysex end outside sysex context
                let packet = self.builder.end_sysex(self.cable_number)?;
                self.builder = PacketBuilder::new();
                Ok(Some(packet))
            }
            (_, SYSEX_END, None, _) => {
                // ignore sysex end outside sysex context
                Ok(None)
            }
            (_, byte, Some(_), _) if is_midi_status(byte) => {
                Err(MidiError::SysexInterrupted)
            }
            (0, byte, Some(pending), _) => {
                // continue sysex stream, release previous packet normally
                self.builder.push(byte);
                self.builder.pending_sysex = None;
                Ok(Some(pending.build(self.cable_number)?))
            }
            (0, byte, None, Some(prev_status)) if !is_midi_status(byte) => {
                // repeating status (MIDI protocol optimisation)
                self.builder.push(prev_status);
                self.builder.push(byte);
                self.release_if_complete(1)
            }
            (0, byte, None, None) if !is_midi_status(byte) => {
                // first byte of non-sysex payload should be a status byte
                Err(MidiError::NotAMidiStatus(byte))
            }
            (0, byte, None, _) if is_midi_status(byte) => {
                // regular status byte starting new command
                self.builder.prev_status = Some(byte);
                self.builder.push(byte);
                self.release_if_complete(1)
            }
            (3, _, _, _) => {
                // can't add more, packet should have been released, probable bug in impl
                Err(MidiError::PayloadOverflow)
            }
            (2, _, None, Some(SYSEX_START)) => {
                // sysex packet complete, wait for next byte before release with correct CIN
                self.builder.push(byte);
                self.builder = PacketBuilder::next(self.builder.prev_status, Some(*self.builder));
                Ok(None)
            }
            (payload_len, _, None, _) if payload_len > 1 => {
                // release current packet if complete
                self.builder.push(byte);
                self.release_if_complete(payload_len)
            }
            _ => {
                Err(MidiError::UnhandledDecode)
            }
        }
    }

    fn release_if_complete(&mut self, payload_len: usize) -> Result<Option<MidiPacket>, MidiError> {
        let cmd_len = self.builder.cmd_len()?;
        if let Some(cmd_len) = cmd_len {
            if cmd_len == payload_len {
                let packet = self.builder.build(self.cable_number)?;
                self.builder = PacketBuilder::next(self.builder.prev_status, None);
                return Ok(Some(packet));
            }
        }
        Ok(None)
    }
}

impl<RX, E> Receive for SerialMidiIn<RX>
    where RX: serial::Read<u8, Error=E>
{
    fn receive(&mut self) -> Result<Option<MidiPacket>, MidiError> {
        let byte = self.serial_in.read()?;
        match self.advance(byte) {
            Err(err) => {
                // TODO record error
                // reset builder & retry with same byte
                rprintln!("Serial MIDI error: {:?}", err);
                self.builder = PacketBuilder::new();
                self.advance(byte)
            }
            packet => packet
        }
    }
}


pub struct SerialMidiOut<TX> {
    serial_out: TX,
    last_status: Option<u8>,
}

impl<TX> SerialMidiOut<TX>
    where TX: serial::Write<u8>
{
    pub fn new(tx: TX) -> Self {
        SerialMidiOut {
            serial_out: tx,
            last_status: None,
        }
    }
}

impl<TX> Transmit for SerialMidiOut<TX>
    where TX: serial::Write<u8>
{
    fn transmit(&mut self, event: MidiPacket) -> Result<(), MidiError> {
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

        'send_payload:
        for byte in payload {
            let mut tries = 0;
            'blocking_write:
            loop {
                match self.serial_out.write(*byte) {
                    Err(nb::Error::WouldBlock) => {
                        tries += 1;
                        if tries > 10000 {
                            rprintln!("Write failed, Serial port _still_ in use after many retries");
                            break 'send_payload
                        }
                    }
                    Err(_err) => {
                        rprintln!("Failed to write serial payload for reason other than blocking");
                        break 'send_payload
                    }
                    _ => break 'blocking_write
                }
            }
        }
        Ok(())
    }
}
