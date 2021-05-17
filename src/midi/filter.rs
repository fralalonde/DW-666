use crate::midi::{Matcher, Message, MidiError};
use crate::midi::route::{RouteContext};
use core::convert::TryFrom;

pub fn capture_sysex(matcher: &mut Matcher, context: &mut RouteContext) -> Result<bool, MidiError> {
    for p in &context.packets {
        if let Some(tags) = matcher.match_packet(*p) {
            context.tags.extend(tags)
        }
    }
    Ok(true)
}

pub fn event_print(context: &mut RouteContext) -> Result<bool, MidiError> {
    for p in &context.packets {
        if let Ok(message) = Message::try_from(*p) {
            match message {
                Message::SysexBegin(byte1, byte2) => rprint!("Sysex [ 0x{:x}, 0x{:x}", byte1, byte2),
                Message::SysexCont(byte1, byte2, byte3) => rprint!(", 0x{:x}, 0x{:x}, 0x{:x}", byte1, byte2, byte3),
                Message::SysexEnd => rprintln!(" ]"),
                Message::SysexEnd1(byte1) => rprintln!(", 0x{:x} ]", byte1),
                Message::SysexEnd2(byte1, byte2) => rprintln!(", 0x{:x}, 0x{:x} ]", byte1, byte2),
                message => if let Some(ch) = p.channel() {
                    rprintln!("ch:{:x?} {:x?}", ch, message)
                } else {
                    rprintln!("{:x?}", message)
                }
            }
        }
    }
    Ok(true)
}

