use crate::midi::{Channel, Matcher, U4,  RouteBinding, Message};
use heapless::Vec;
use crate::midi::route::{RoutingContext, RouterEvent};
use core::convert::TryFrom;

/// Enum used as object class discriminant.
/// Mutable object state held inside each.
#[derive(Debug)]
pub enum Filter {
    Print,
    OnlyChannel(Channel),
    SysexCapture(Matcher),
}

impl Filter {
    pub fn apply(&mut self, context: &mut RoutingContext) -> bool {
        for event in context.events() {
            if !match self {
                Filter::OnlyChannel(only) => only_channel(event, only),
                Filter::Print => filter_print(event),
                Filter::SysexCapture(matcher) => sysex_capture(context, event, matcher),
            } {
                return false
            }
        }
        true
    }

    /// Informs router of the event types this filter responds to, if any
    pub fn bindings(&self) -> &'static [RouteBinding] {
        match self {
            _ => &[]
        }
    }
}

fn sysex_capture(context: &mut RoutingContext, event: &RouterEvent, matcher: &mut Matcher) -> bool {
    if let RouterEvent::Packet(_, packet) = event {
        if let Some(tags) = matcher.match_packet(*packet) {
            for t in tags {
                context.set_tag(t.0, t.1)
            }
        }
    }
    true
}

fn only_channel(event: &RouterEvent, only: &mut U4) -> bool {
    if let RouterEvent::Packet(_, packet) = event {
        if let Some(channel) = packet.channel() {
            if channel != *only {
                return false;
            }
        }
    }
    true
}

fn filter_print(event: &RouterEvent) -> bool {
    if let RouterEvent::Packet(_, packet) = event {
        if let Ok(message) = Message::try_from(*packet) {
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