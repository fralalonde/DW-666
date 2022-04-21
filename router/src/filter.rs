use midi::{Message, MidiError};
use crate::route::{RouteContext};
use core::convert::TryFrom;
use crate::sysex::Matcher;

pub fn capture_sysex(matcher: &mut Matcher, context: &mut RouteContext) -> Result<bool, MidiError> {
    for p in context.packets.iter() {
        if let Some(tags) = matcher.match_packet(*p) {
            context.tags.extend(tags)
        }
    }
    Ok(true)
}

/// Print packets to the console and continue
pub fn print_message(context: &mut RouteContext) -> Result<bool, MidiError> {
    for p in context.packets.iter() {
        if let Ok(message) = Message::try_from(*p) {
            match message {
                Message::SysexBegin(byte1, byte2) => info!("Sysex [ 0x{:x}, 0x{:x}", byte1, byte2),
                Message::SysexCont(byte1, byte2, byte3) => info!(", 0x{:x}, 0x{:x}, 0x{:x}", byte1, byte2, byte3),
                Message::SysexEnd => info!(" ]"),
                Message::SysexEnd1(byte1) => info!(", 0x{:x} ]", byte1),
                Message::SysexEnd2(byte1, byte2) => info!(", 0x{:x}, 0x{:x} ]", byte1, byte2),
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
pub fn _print_packets(context: &mut RouteContext) -> Result<bool, MidiError> {
    for p in context.packets.iter() {
        info!("packet {:?}", p);
    }
    Ok(true)
}

