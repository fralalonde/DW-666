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
    fn apply_filters(&mut self, now: Instant, packet: Packet) -> Option<RouteContext> {
        // TODO reuse context & preallocated packet vec
        let mut context = RouteContext::default();
        context.packets.push(packet);
        for filter in &mut self.filters {
            match (filter)(now, &mut context) {
                Err(e) => rprintln!("Filter error: {:?}", e),
                Ok(false) => return None,
                _ => {}
            }
        }
        Some(context)
    }
}


/// Events that may trigger a route
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RouteBinding {
    Src(Interface),
    Dst(Interface),
    Clock,
}

#[derive(Default)]
pub struct RouteContext {
    pub destination: Option<Interface>,
    pub tags: HashMap<Tag, Vec<u8>>,
    pub packets: Vec<Packet>,
    pub strings: Vec<String>,
}

impl RouteContext {}

type RouteVec = Vec<Handle>;

#[derive(Default)]
pub struct Router {
    exclusive: HashMap<RouteBinding, Handle>,
    bindings: HashMap<RouteBinding, RouteVec>,
    routes: HashMap<Handle, Route>,
    // TODO route ID pooling instead
}

impl Router {
    pub fn dispatch_midi(&mut self, now: Instant, packet: Packet, binding: RouteBinding, spawn: dispatch_midi::Spawn) {
        if let Some(route_id) = self.exclusive.get(&binding).cloned() {
            if let Err(err) = self.dispatch_route_id(route_id, now, packet, spawn) {
                rprintln!("Exclusive Route {} for {:?} dispatch failed: {:?}", route_id, binding, err)
            }
        } else if let Some(route_ids) = self.bindings.get(&binding).cloned() {
            // routes are independent from each other, could be processed concurrently
            for route_id in route_ids {
                if let Err(err) = self.dispatch_route_id(route_id, now, packet, spawn) {
                    rprintln!("Shared Route {} for {:?} dispatch failed: {:?}", route_id, binding, err)
                }
            }
        }
    }

    pub fn dispatch_route_id(&mut self, route_id: Handle, now: Instant, packet: Packet, spawn: dispatch_midi::Spawn) -> Result<(), MidiError> {
        if let Some(route) = self.routes.get_mut(&route_id) {
            if let Some(context) = route.apply_filters(now, packet) {
                // dynamic destination may override route destination
                if let Some(destination) = context.destination.or(route.destination) {
                    for p in context.packets {
                        spawn.send_midi(destination, p)?
                    }
                } else {
                    rprintln!("No destination for route")
                }
                for s in context.strings {
                    spawn.redraw(s)?
                }
            }
        } else {
            rprintln!("Route ID {} triggered but not found", route_id)
        }
        Ok(())
    }

    pub fn add_route(&mut self, route: Route) -> Result<Handle, MidiError> {
        let route_id = NEXT_HANDLE.fetch_add(1, Relaxed);

        if route.exclusive {
            if let Some(src) = route.source {
                self.bind_exclusive(&Src(src), route_id)?;
            } else if let Some(dst) = route.destination {
                self.bind_exclusive(&Dst(dst), route_id)?;
            }
        } else {
            if let Some(src) = route.source {
                self.bind_shared(&Src(src), route_id);
            } else if let Some(dst) = route.destination {
                self.bind_shared(&Dst(dst), route_id);
            }
        }

        self.routes.insert(route_id, route);
        Ok(route_id)
    }

    fn bind_exclusive(&mut self, binding: &RouteBinding, route_id: Handle) -> Result<(), MidiError> {
        if let Some(existing) = self.exclusive.get_mut(binding) {
            Err(MidiError::ExclusiveRouteConflict(*existing))
        } else {
            self.exclusive.insert(*binding, route_id);
            Ok(())
        }
    }

    fn bind_shared(&mut self, binding: &RouteBinding, route_id: Handle) {
        if let Some(route_ids) = self.bindings.get_mut(binding) {
            route_ids.push(route_id);
        } else {
            let mut route_ids: RouteVec = Vec::new();
            route_ids.push(route_id);
            self.bindings.insert(*binding, route_ids);
        }
    }

    pub fn unbind(&mut self, route_id: Handle) {
        let removed = self.routes.remove(&route_id);
        if let Some(route) = removed {
            if let Some(src) = route.source {
                self.try_remove(route_id, &Src(src));
            }
            if let Some(dst) = route.destination {
                self.try_remove(route_id, &Dst(dst));
            }
        }
    }

    fn try_remove(&mut self, route_id: Handle, bin: &RouteBinding) {
        if let Some(bins) = self.bindings.get_mut(bin) {
            if let Some((idx, _)) = bins.iter().enumerate().find(|(_i, v)| **v == route_id) {
                bins.swap_remove(idx);
            } else {
                rprintln!("Route id {} not found in bindings {:?} index: {:?}", route_id, bin, bins)
            }
        } else {
            rprintln!("Route has source {:?} but is bindings is empty", bin)
        }
    }
}

