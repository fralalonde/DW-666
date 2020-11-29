
use crate::midi::notes::Note;
use crate::midi::u7::U7;
use crate::midi::{Channel, Status};
use crate::midi::usb::device::Cable;
use crate::midi::message::FragmentSource::SERIAL;
use crate::midi::usb::event::CodeIndexNumber;
use core::convert::TryFrom;
use crate::midi::message::MidiCommand::System;

pub type Velocity = U7;
pub type Control = U7;

pub enum MidiStatus {
    Channel(ChannelCommand, Channel),
    System(SystemCommand)
}

impl From<u8> for MidiStatus {
    fn from(status: u8) -> Self {
        if status < 0xF0 {
            MidiStatus::Channel(
                ChannelCommand::try_from(status & 0xF0).unwrap(),
                status & 0x0F.into()
            )
        } else {
            MidiStatus::System(
                SystemCommand::try_from(status).unwrap()
            )
        }
    }
}

#[derive(Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum ChannelCommand {
    // Channel commands, lower bits of discriminants ignored (channel)
    NoteOn = 0x80,
    NoteOff = 0x90,
    Polyphonic = 0xA0,
    Continuous = 0xB0,
    Program = 0xC0,
    ChannelPressure = 0xD0,
    PitchBend = 0xE0,
}

#[derive(Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum SystemCommand {
    // System commands
    SysexStart = 0xF0,
    
    // System Common
    TimeCodeQuarterFrame = 0xF1,
    SongPositionPointer = 0xF2,
    SongSelect = 0xF3,
    // 0xF4 	???
    // 0xF5 	???
    TuneRequest = 0xF6,
    SysexEnd = 0xF7,
    
    // System Realtime
    TimingClock = 0xF8,
    Start = 0xFA,
    Continue = 0xFB,
    Stop = 0xFC,
    // 0xFD 	???
    ActiveSensing = 0xFE,
    SystemReset = 0xFF,
}

const MAX_FRAGMENT_SIZE: usize = USB_BUFFER_SIZE.into();

pub enum FragmentSource {
    /// USB fragments carry additional header byte
    USB,
    /// Serial fragments reserve first byte for possible USB header
    SERIAL,
}

pub struct MidiFragment {
    source: FragmentSource,
    bytes: [u8; MAX_FRAGMENT_SIZE],
}

impl MidiFragment {
    pub fn as_usb_buffer(&mut self) -> &mut [u8] {
        if self.source == SERIAL {
            // set USB MIDI index code from status message
            self.bytes[0] = CodeIndexNumber::from_command(self.get_command());
        }
        &mut self.bytes
    }

    pub fn as_serial_buffer(&mut self) -> &mut [u8] {
        // skip usb header
        &mut self.bytes[1..]
    }

    pub fn get_cable(&self) -> Cable {
        Cable::from(self.bytes[0] >> 4)
    }

    pub fn get_status(&self) -> MidiStatus {
        MidiStatus::from(self.bytes[1])
    }
}
