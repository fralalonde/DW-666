use crate::midi::{Packet, Tag, Interface, MidiError};
use self::RouteBinding::*;

use alloc::vec::Vec;
use hashbrown::{HashMap};

use core::sync::atomic::Ordering::Relaxed;

use crate::{dispatch_midi, Handle, NEXT_HANDLE};
use rtic::cyccnt::{Instant};
use alloc::boxed::Box;
use core::fmt::{Debug};
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
    exclusive: bool,
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

    /// Route A -> B
    pub fn exclusive(mut self) -> Self {
        self.exclusive = true;
        self
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


/// Events that may trigger a route
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RouteBinding {
    Src(Interface),
    Dst(Interface),
}

#[derive(Default, Clone)]
pub struct RouteContext {
    pub destination: Option<Interface>,
    pub tags: HashMap<Tag, Vec<u8>>,
    pub packets: Vec<Packet>,
    pub strings: Vec<String>,
}

impl RouteContext {
    fn restart(&mut self, packet: Packet) {
        self.destination = None;
        self.tags.clear();
        self.packets = vec![packet];
        self.strings = vec![];
    }

    fn flush_strings(&mut self, spawn: dispatch_midi::Spawn) -> Result<(), MidiError> {
        for s in self.strings.drain(..) {
            if let Err(e) = spawn.redraw(s) {
                rprintln!("Failed enqueue redraw: {}", e)
            }
        }
        Ok(())
    }

    fn flush_packets(&mut self, destination: Interface, spawn: dispatch_midi::Spawn) -> Result<(), MidiError> {
        for p in self.packets.drain(..) {
            spawn.send_midi(destination, p)?
        }
        Ok(())
    }
}

type RouteVec = BTreeMap<Handle, Route>;

#[derive(Default)]
pub struct Router {
    // exclusive: HashMap<RouteBinding, Handle>,
    ingress: HashMap<Interface, RouteVec>,
    egress: HashMap<Interface, RouteVec>,
    // routes: HashMap<Handle, Route>,
    // TODO route ID pooling instead
}

impl Router {
    pub fn dispatch_midi(&mut self, now: Instant, packet: Packet, binding: RouteBinding, spawn: dispatch_midi::Spawn) -> Result<(), MidiError> {
        // TODO preallocate static context
        let mut context = RouteContext::default();

        // TODO handle immediate Dst routes
        if let Src(source) = binding {
            if let Some(routes) = self.ingress.get_mut(&source) {
                for route_in in routes.values_mut() {
                    context.restart(packet);
                    if route_in.apply_filters(now, &mut context) {
                        context.flush_strings(spawn)?;
                        if let Some(destination) = context.destination.or(route_in.destination) {
                            if let Some(routes) = self.egress.get_mut(&destination) {
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
                        } else {
                            rprintln!("No destination route")
                        }
                    }
                }
            }
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

    // fn bind_shared(&mut self, routes: &mut HashMap<Interface, RouteVec>, interface: Interface, route_id: Handle, route: Route) {}

    // pub fn unbind(&mut self, route_id: Handle) {
    //     let removed = self.routes.remove(&route_id);
    //     if let Some(route) = removed {
    //         if let Some(src) = route.source {
    //             self.try_remove(route_id, &Src(src));
    //         }
    //         if let Some(dst) = route.destination {
    //             self.try_remove(route_id, &Dst(dst));
    //         }
    //     }
    // }

    // fn try_remove(&mut self, route_id: Handle, bin: &RouteBinding) {
    //     if let Some(bins) = self.bindings.get_mut(bin) {
    //         if let Some((idx, _)) = bins.iter().enumerate().find(|(_i, v)| **v == route_id) {
    //             bins.swap_remove(idx);
    //         } else {
    //             rprintln!("Route id {} not found in bindings {:?} index: {:?}", route_id, bin, bins)
    //         }
    //     } else {
    //         rprintln!("Route has source {:?} but is bindings is empty", bin)
    //     }
    // }
}

