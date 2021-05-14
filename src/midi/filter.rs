use crate::midi::{Matcher, Message};
use crate::midi::route::{RouteContext, RouterEvent};
use core::convert::TryFrom;

pub fn capture_sysex(matcher: &mut Matcher, event: RouterEvent, context: &mut RouteContext) -> bool {
    if let RouterEvent::Packet(packet) = event {
        if let Some(tags) = matcher.match_packet(packet) {
            context.tags.extend(tags)
        }
    }
    true
}

pub fn event_print(event: RouterEvent, _context: &mut RouteContext) -> bool {
    if let RouterEvent::Packet(packet) = event {
        if let Ok(message) = Message::try_from(packet) {
            match message {
                Message::SysexBegin(byte1, byte2) => rprint!("Sysex [ 0x{:x}, 0x{:x}", byte1, byte2),
                Message::SysexCont(byte1, byte2, byte3) => rprint!(", 0x{:x}, 0x{:x}, 0x{:x}", byte1, byte2, byte3),
                Message::SysexEnd => rprintln!(" ]"),
                Message::SysexEnd1(byte1) => rprintln!(", 0x{:x} ]", byte1),
                Message::SysexEnd2(byte1, byte2) => rprintln!(", 0x{:x}, 0x{:x} ]", byte1, byte2),
                message => if let Some(ch) = packet.channel() {
                    rprintln!("ch:{:x?} {:x?}", ch, message)
                } else {
                    rprintln!("{:x?}", message)
                }
            }
        }
    }
    true
}

