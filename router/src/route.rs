use midi::{Interface, MidiError, Binding, PacketList};

use alloc::vec::Vec;
use hashbrown::{HashMap};

use core::sync::atomic::Ordering::Relaxed;

use crate::{Handle, midi_send, midisplay, NEXT_HANDLE};

use alloc::boxed::Box;
use alloc::string::String;
use alloc::collections::{BTreeMap};
use crate::sysex::Tag;

pub trait Service {
    fn start(&mut self) -> Result<(), MidiError>;
}

#[derive(Default)]
pub struct Route {
    source: Option<Interface>,
    destination: Option<Interface>,
    filters: Vec<Box<dyn FnMut(&mut RouteContext) -> Result<bool, MidiError> + Send + 'static>>,
}

impl Route {
    /// Route A -> *
    pub fn from(interface: Interface) -> Self {
        Route { source: Some(interface), ..Default::default() }
    }

    /// Route * -> A
    pub fn to(interface: Interface) -> Self {
        Route { destination: Some(interface), ..Default::default() }
    }

    /// Route A -> B
    pub fn link(from: Interface, to: Interface) -> Self {
        let mut route = Route::from(from);
        route.destination = Some(to);
        route
    }

    /// Route A -> A
    pub fn echo(interface: Interface) -> Self {
        Route::link(interface, interface)
    }

    /// Routes A -> B and B -> A
    pub fn circuit(interface1: Interface, interface2: Interface) -> (Self, Self) {
        (Self::link(interface1, interface2), Route::link(interface2, interface1))
    }

    pub fn filter<F>(mut self, filter: F) -> Self
        where F: FnMut(&mut RouteContext) -> Result<bool, MidiError> + Send + 'static
    {
        self.filters.push(Box::new(filter));
        self
    }

    /// Return true if router should forward event to destinations
    /// Return false to discard the event
    /// Does not affect other routes
    fn apply_filters(&mut self, context: &mut RouteContext) -> bool {
        for filter in &mut self.filters {
            match (filter)(context) {
                Err(e) => info!("Filter error: {:?}", e),
                Ok(false) => return false,
                _ => {}
            }
        }
        true
    }
}

#[derive(Default, Clone)]
pub struct RouteContext {
    pub destination: Option<Interface>,
    pub tags: HashMap<Tag, Vec<u8>>,
    pub packets: PacketList,
    pub strings: Vec<String>,
}

impl RouteContext {
    fn restart(&mut self, packets: PacketList) {
        self.destination = None;
        self.tags.clear();
        self.packets = packets;
        self.strings = vec![];
    }

    fn flush_strings(&mut self) -> Result<(), MidiError> {
        for s in self.strings.drain(..) {
            midisplay(s)
        }
        Ok(())
    }

    fn flush_packets(&mut self, destination: Interface) -> Result<(), MidiError> {
        midi_send(destination, self.packets.clone());
        // heapless Vec has no drain() method :(
        self.packets.clear();
        Ok(())
    }
}

type RouteVec = BTreeMap<Handle, Route>;

#[derive(Default)]
pub struct Router {
    ingress: HashMap<Interface, RouteVec>,
    egress: HashMap<Interface, RouteVec>,
}

impl Router {
    pub fn midi_route(&mut self, packets: PacketList, binding: Binding) -> Result<(), MidiError> {
        // TODO preallocate static context
        let mut context = RouteContext::default();

        match binding {
            Binding::Dst(destination) => {
                context.restart(packets);
                Self::out(&mut self.egress, &mut context, destination)?
            }
            Binding::Src(source) =>
                if let Some(routes) = self.ingress.get_mut(&source) {
                    for route_in in routes.values_mut() {
                        context.restart(packets.clone());
                        if route_in.apply_filters(&mut context) {
                            context.flush_strings()?;
                            if let Some(destination) = context.destination.or(route_in.destination) {
                                Self::out(&mut self.egress, &mut context, destination)?
                            }
                        }
                    }
                }
        }
        Ok(())
    }

    fn out(egress: &mut HashMap<Interface, RouteVec>, context: &mut RouteContext, destination: Interface) -> Result<(), MidiError> {
        if let Some(routes) = egress.get_mut(&destination) {
            for route_out in routes.values_mut() {
                // isolate out routes from each other
                let mut context = context.clone();
                if route_out.apply_filters(&mut context) {
                    context.flush_packets(destination)?;
                    context.flush_strings()?;
                }
            }
        } else {
            // no destination route, just send
            context.flush_packets(destination)?;
        }

        Ok(())
    }

    pub fn add_route(&mut self, route: Route) -> Result<Handle, MidiError> {
        let route_id = NEXT_HANDLE.fetch_add(1, Relaxed);

        if let Some(src) = route.source {
            self.ingress.entry(src).or_default().insert(route_id, route);
        } else if let Some(dst) = route.destination {
            self.egress.entry(dst).or_default().insert(route_id, route);
        }

        Ok(route_id)
    }
}

