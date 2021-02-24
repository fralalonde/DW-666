//! *Midi driver on top of embedded hal serial communications*
//!
use crate::midi::status::{SYSEX_START, SYSEX_END, is_midi_status};
use embedded_hal::serial;
use crate::midi::packet::{MidiPacket, CableNumber, PacketBuilder};
use crate::midi::{MidiError, Receive, Transmit};
use crate::midi::status::SystemCommand::SysexStart;

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

    fn release_if_complete(&mut self, payload_len: u8) -> Result<Option<MidiPacket>, MidiError> {
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
    fn transmit(&mut self, event: MidiPacket)  -> Result<(), MidiError> {
        let mut payload = event.payload()?;
        let new_status = Some(payload[0]);
        if self.last_status == new_status {
            payload = &payload[1..];
        } else {
            self.last_status = new_status;
        }

        for byte in payload {
            self.serial_out.write(*byte).unwrap_err();
        };
        Ok(())
    }
}
