use midi::{MidiMessage, MidiError, PacketList};

use core::convert::TryFrom;

/// Print packets to the console and continue
pub fn print_message(packets: &PacketList) -> Result<bool, MidiError> {
    for p in packets.iter() {
        if let Ok(message) = MidiMessage::try_from(*p) {
            match message {
                MidiMessage::SysexBegin(byte1, byte2) => info!("Sysex [ 0x{:x}, 0x{:x}", byte1, byte2),
                MidiMessage::SysexCont(byte1, byte2, byte3) => info!(", 0x{:x}, 0x{:x}, 0x{:x}", byte1, byte2, byte3),
                MidiMessage::SysexEnd => info!(" ]"),
                MidiMessage::SysexEnd1(byte1) => info!(", 0x{:x} ]", byte1),
                MidiMessage::SysexEnd2(byte1, byte2) => info!(", 0x{:x}, 0x{:x} ]", byte1, byte2),
                message => if let Some(ch) = p.channel() {
                    info!("ch:{:?} {:?}", ch, message)
                } else {
                    info!("{:?}", message)
                }
            }
        }
    }
    Ok(true)
}

/// Print packets to the console and continue
pub fn print_packets(packets: &PacketList) -> Result<bool, MidiError> {
    for p in packets.iter() {
        info!("packet {:?}", p);
    }
    Ok(true)
}

