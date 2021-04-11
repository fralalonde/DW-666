use crate::midi::{Channel, ResponseMatcher, U4, RouteBinding, Message};
use crate::midi::route::{RouteContext, RouterEvent};
use core::convert::TryFrom;

/// Enum used as object class discriminant.
/// Mutable object state held inside each.
#[derive(Debug)]
pub enum Filter {
    PrintEvent,
    PrintTags,
    OnlyChannel(Channel),
    SysexCapture(ResponseMatcher),
}

impl Filter {
    /// Poor man's dynamic dispatch
    /// Filters could become trait objects if alloc is used
    pub fn apply(&mut self, event: RouterEvent, context: &mut RouteContext) -> bool {
        match self {
            Filter::OnlyChannel(only) => only_channel(event, only),
            Filter::PrintEvent => print_event(event),
            Filter::PrintTags => print_tags(context),
            Filter::SysexCapture(matcher) => sysex_capture(context, event, matcher),
        }
    }

    /// Informs router of the event types this filter responds to, if any
    pub fn bindings(&self) -> &'static [RouteBinding] {
        match self {
            _ => &[]
        }
    }
}

fn sysex_capture(context: &mut RouteContext, event: RouterEvent, matcher: &mut ResponseMatcher) -> bool {
    if let RouterEvent::Packet(_, packet) = event {
        if let Some(tags) = matcher.match_packet(packet) {
            for t in tags {
                context.add_tag_value(t.0, t.1)
            }
        }
    }
    true
}

fn only_channel(event: RouterEvent, only: &mut U4) -> bool {
    if let RouterEvent::Packet(_, packet) = event {
        if let Some(channel) = packet.channel() {
            if channel != *only {
                return false;
            }
        }
    }
    true
}

fn print_event(event: RouterEvent) -> bool {
    if let RouterEvent::Packet(_, packet) = event {
        if let Ok(message) = Message::try_from(packet) {
            match message {
                Message::SysexBegin(byte1, byte2) => rprint!("Sysex [ 0x{:x}, 0x{:x}", byte1, byte2),
                Message::SysexCont(byte1, byte2, byte3) => rprint!(", 0x{:x}, 0x{:x}, 0x{:x}", byte1, byte2, byte3),
                Message::SysexEnd => rprintln!(" ]"),
                Message::SysexEnd1(byte1) => rprintln!(", 0x{:x} ]", byte1),
                Message::SysexEnd2(byte1, byte2) => rprintln!(", 0x{:x}, 0x{:x} ]", byte1, byte2),
                message => rprintln!("{:?}", message)
            }
        }
    }
    true
}

fn print_tags(context: &mut RouteContext) -> bool {
    if !context.tags.is_empty() {
        rprintln!("Context Tags {:?}", context.tags);
    }
    true
}