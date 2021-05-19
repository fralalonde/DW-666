use crate::midi::{Packet, Tag, Interface, MidiError, Binding};

use alloc::vec::Vec;
use hashbrown::{HashMap};

use core::sync::atomic::Ordering::Relaxed;

use crate::{midispatch, Handle, NEXT_HANDLE};
use rtic::cyccnt::{Instant};
use alloc::boxed::Box;
use crate::time::Tasks;
use alloc::string::String;
use alloc::collections::{BTreeMap};

pub trait Service {
    fn start(&mut self, now: rtic::cyccnt::Instant, router: &mut Router, tasks: &mut Tasks) -> Result<(), MidiError>;
    fn stop(&mut self, _router: &mut Router) {}
}

#[derive(Default)]
pub struct Route {
    source: Option<Interface>,
    destination: Option<Interface>,
    filters: Vec<Box<dyn FnMut(Instant, &mut RouteContext) -> Result<bool, MidiError> + Send + 'static>>,
}

impl Route {
    /// Route A -> *
    pub fn from(interface: Interface) -> Self {
        let mut route = Route::default();
        route.source = Some(interface);
        route
    }

    /// Route * -> A
    pub fn to(interface: Interface) -> Self {
        let mut route = Route::default();
        route.destination = Some(interface);
        route
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
        where F: FnMut(Instant, &mut RouteContext) -> Result<bool, MidiError> + Send + 'static
    {
        self.filters.push(Box::new(filter));
        self
    }

    /// Return true if router should forward event to destinations
    /// Return false to discard the event
    /// Does not affect other routes
    fn apply_filters(&mut self, now: Instant, context: &mut RouteContext) -> bool {
        for filter in &mut self.filters {
            match (filter)(now, context) {
                Err(e) => rprintln!("Filter error: {:?}", e),
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
    pub packets: Vec<Packet>,
    pub strings: Vec<String>,
}

impl RouteContext {
    fn restart(&mut self, packets: Vec<Packet>) {
        self.destination = None;
        self.tags.clear();
        self.packets = packets;
        self.strings = vec![];
    }

    fn flush_strings(&mut self, spawn: midispatch::Spawn) -> Result<(), MidiError> {
        for s in self.strings.drain(..) {
            if let Err(e) = spawn.redraw(s) {
                rprintln!("Failed enqueue redraw {}", e)
            }
        }
        Ok(())
    }

    fn flush_packets(&mut self, destination: Interface, spawn: midispatch::Spawn) -> Result<(), MidiError> {
        spawn.send_midi(destination, self.packets.drain(..).collect())?;
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
    pub fn midispatch(&mut self, now: Instant, packets: Vec<Packet>, binding: Binding, spawn: midispatch::Spawn) -> Result<(), MidiError> {
        // TODO preallocate static context
        let mut context = RouteContext::default();

        match binding {
            Binding::Dst(destination) =>
                Self::out(&mut self.egress, now, spawn, &mut context, destination)?,
            Binding::Src(source) =>
                if let Some(routes) = self.ingress.get_mut(&source) {
                    for route_in in routes.values_mut() {
                        context.restart(packets.clone());
                        if route_in.apply_filters(now, &mut context) {
                            context.flush_strings(spawn)?;
                            if let Some(destination) = context.destination.or(route_in.destination) {
                                Self::out(&mut self.egress, now, spawn, &mut context, destination)?
                            } else {
                                rprintln!("No destination route")
                            }
                        }
                    }
                }
        }
        Ok(())
    }

    fn out(egress: &mut HashMap<Interface, RouteVec>, now: Instant, spawn: midispatch::Spawn, context: &mut RouteContext, destination: Interface) -> Result<(), MidiError> {
        if let Some(routes) = egress.get_mut(&destination) {
            for route_out in routes.values_mut() {
                // isolate out routes from each other
                let mut context = context.clone();
                if route_out.apply_filters(now, &mut context) {
                    context.flush_packets(destination, spawn)?;
                    context.flush_strings(spawn)?;
                }
            }
        } else {
            // no destination route, just send
            context.flush_packets(destination, spawn)?;
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

