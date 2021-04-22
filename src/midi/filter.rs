use crate::midi::{Matcher, U4, RouteBinding, Message};
use crate::midi::route::{RouteContext, RouterEvent};
use core::convert::TryFrom;
use core::fmt::Debug;
use alloc::boxed::Box;

pub trait Filter: Debug + Send {
    /// Apply the filter logic
    fn apply(&mut self, event: RouterEvent, context: &mut RouteContext) -> bool;

    /// Informs router of additional event types the filtered route should is interested in
    fn bindings(&self) -> &'static [RouteBinding] { &[] }
}

#[derive(Debug)]
struct SysexCapture {
    pub matcher: Matcher,
}

impl Filter for SysexCapture {
    fn apply(&mut self, event: RouterEvent, context: &mut RouteContext) -> bool {
        if let RouterEvent::Packet(packet) = event {
            if let Some(tags) = self.matcher.match_packet(packet) {
                context.tags.extend(tags)
            }
        }
        true
    }
}

pub fn capture_sysex(matcher: Matcher) -> Box<dyn Filter> {
    Box::new(SysexCapture { matcher })
}

#[derive(Debug)]
struct TagPrint();

impl Filter for TagPrint {
    fn apply(&mut self, _event: RouterEvent, context: &mut RouteContext) -> bool {
        if !context.tags.is_empty() {
            rprintln!("Context Tags {:?}", context.tags);
        }
        true
    }
}

pub fn print_tag() -> Box<dyn Filter> {
    Box::new(TagPrint())
}

#[derive(Debug)]
struct EventPrint();

impl Filter for EventPrint {
    fn apply(&mut self, event: RouterEvent, _context: &mut RouteContext) -> bool {
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
                        rprintln ! ("{:x?}", message)
                    }
                }
            }
        }
        true
    }
}

pub fn event_print() -> Box<dyn Filter> {
    Box::new(EventPrint())
}

// #[derive(Debug)]
// struct KeepNotes {}
//
// impl Filter for KeepNotes {
//     fn apply(&mut self, event: RouterEvent, _context: &mut RouteContext) -> bool {
//         if let RouterEvent::Packet(packet) = event {
//             match Message::try_from(packet) {
//                 Ok(Message::NoteOn(..)) | Ok(Message::NoteOff(..)) => return true,
//                 _ => {}
//             }
//         }
//         false
//     }
// }
//
// pub fn keep_notes() -> Box<dyn Filter> {
//     Box::new(KeepNotes {})
// }


fn only_channel(event: RouterEvent, only: &mut U4) -> bool {
    if let RouterEvent::Packet(packet) = event {
        if let Some(channel) = packet.channel() {
            if channel != *only {
                return false;
            }
        }
    }
    true
}
