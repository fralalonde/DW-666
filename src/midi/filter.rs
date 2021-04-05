use crate::midi::{Channel, SysexMatcher, Packet, U4, Cull, RouteBinding};
use alloc::vec::Vec;
use crate::midi::route::{RoutingContext, RouterEvent};

/// Enum used as object class discriminant.
/// Mutable object state held inside each.
#[derive(Debug)]
pub enum Filter {
    FilterChannel(Channel),
    // TODO more transforms...
    MessageToSysex {
        z: u32
    },
    SysexToMessage(SysexMatcher, fn()),
    Arpeggiator,
}

impl Filter {
    pub fn apply(&mut self, context: &mut RoutingContext) -> bool {
        for event in context.events() {
            match self {
                Filter::FilterChannel(only) => {
                    if let RouterEvent::Packet(_, packet) = event {
                        if let Some(channel) = packet.channel() {
                            if channel != *only {
                                // *only = U4::cull(5);
                                return false;
                            }
                        }
                    }
                }
                _ => {
                    todo!()
                }
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