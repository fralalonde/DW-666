use heapless::{FnvIndexMap, Vec, FnvIndexSet};
use crate::midi::{Packet, Filter, Tag, U7};
use self::RouteBinding::*;
use hash32;
use core::sync::atomic::AtomicU16;
use core::sync::atomic::Ordering::Relaxed;
use crate::event::{Duration};


#[derive(Debug, Default)]
pub struct Route {
    priority: u8,
    source: Option<Interface>,
    destination: Option<Interface>,
    filters: Vec<Filter, 4>,
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

    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Return true if router should forward event to destinations
    /// Return false to discard the event
    /// Does not affect other routes
    fn apply(&mut self, event: RouterEvent) -> bool {
        let mut context = RouteContext::default();
        for filter in &mut self.filters {
            if !filter.apply(event, &mut context) {
                return false;
            }
        }
        true
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Interface {
    USB,
    Serial(u8),
    // TODO virtual interfaces ?
}

impl hash32::Hash for Interface {
    fn hash<H: hash32::Hasher>(&self, state: &mut H) {
        match self {
            Interface::USB => state.write(&[0]),
            Interface::Serial(idx) => state.write(&[0xF + idx])
        }
    }
}


/// Events that may trigger a route
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RouteBinding {
    Src(Interface),
    Dst(Interface),
    Clock,
}

impl hash32::Hash for RouteBinding {
    fn hash<H: hash32::Hasher>(&self, state: &mut H) {
        match self {
            Src(idx) => {
                state.write(&[0x4]);
                idx.hash(state);
            }
            Dst(idx) => {
                state.write(&[0x8]);
                idx.hash(state);
            }
            Clock => {
                state.write(&[0xA])
            }
        }
    }
}

pub type RouteId = u16;

#[derive(Debug, Copy, Clone)]
pub enum RouterEvent {
    /// Original packet gets time "now" by default
    /// Packets can be scheduled to be sent in the future with Duration > 0
    Packet(Duration, Packet),
    /// Clock events are always "now"
    Clock,
}

#[derive(Debug, Default)]
pub struct RouteContext {
    // new_events: Vec<RouterEvent, 64>,
    destinations: FnvIndexSet<Interface, 4>,
    pub tags: FnvIndexMap<Tag, Vec<U7, 4>, 4>,
}

impl RouteContext {
    pub fn add_destination(&mut self, destination: Interface) {
        if let Err(_e) = self.destinations.insert(destination) {
            rprintln!("Destination dropped: Routing buffer full")
        }
    }

    pub fn add_tag_value(&mut self, tag: Tag, value: U7) {
        if let Some(mut values) = self.tags.get_mut(&tag) {
            values.push(value);
        } else {
            let mut values = Vec::new();
            values.push(value);
            self.tags.insert(tag, values);
        }
    }
}

type RouteVec = Vec<RouteId, 8>;

#[derive(Debug, Default)]
pub struct Router {
    enabled: bool,
    bindings: FnvIndexMap<RouteBinding, RouteVec, 16>,
    routes: FnvIndexMap<RouteId, Route, 64>,
    // TODO route ID pooling instead
    next_route_id: AtomicU16,
    // context: RoutingContext,
    bug: RouteVec,
}

use crate::dispatch_from;
use rtic::cyccnt::{Instant, U32Ext};

impl Router {
    pub fn dispatch_from(&mut self, now: Instant, packet: Packet, source: Interface, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.bindings.get(&Src(source)).cloned() {
            self.dispatch(now, RouterEvent::Packet(0, packet), &route_ids, spawn, schedule)
        }
    }

    pub fn dispatch_to(&mut self, now: Instant, packet: Packet, destination: Interface, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.bindings.get(&Dst(destination)).cloned() {
            self.dispatch(now, RouterEvent::Packet(0, packet), &route_ids, spawn, schedule)
        }
    }

    pub fn dispatch_clock(&mut self, now: Instant, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.bindings.get(&Clock).cloned() {
            self.dispatch(now, RouterEvent::Clock, &route_ids, spawn, schedule)
        }
    }

    fn dispatch(&mut self, now: Instant, event: RouterEvent, route_ids: &RouteVec, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        let routes = &mut self.routes;
        for route_id in route_ids {
            if let Some(route) = routes.get_mut(route_id) {
                if route.apply(event) {
                    if let Some(destination) = route.destination {
                        if let RouterEvent::Packet(delay, packet) = event {
                            if delay == 0 {
                                spawn.send_midi(destination, packet).unwrap()
                            } else {
                                // quantized or delayed => send later
                                // FIXME duration should NOT be in cycles
                                schedule.send_midi(now + delay.cycles(), destination, packet).unwrap()
                            }
                        }
                    }
                }
            } else {
                rprintln!("Route ID {} triggered but not found", route_id)
            }
        }
    }

    pub fn bind(&mut self, route: Route) -> RouteId {
        let route_id = self.next_route_id.fetch_add(1, Relaxed);

        if let Some(src) = route.source {
            self.bind_route(&Src(src), route_id);
        }

        if let Some(dst) = route.destination {
            self.bind_route(&Dst(dst), route_id);
        }

        for f in &route.filters {
            for b in f.bindings() {
                self.bind_route(b, route_id);
            }
        }

        self.routes.insert(route_id, route);

        route_id
    }

    fn bind_route(&mut self, binding: &RouteBinding, route_id: RouteId) {
        // FIXME heapless fnvmap does not have entry() yet
        if let Some(route_ids) = self.bindings.get_mut(binding) {
            route_ids.push(route_id);
        } else {
            let mut route_ids: RouteVec = Vec::new();
            route_ids.push(route_id);
            self.bindings.insert(*binding, route_ids);
        }
    }

    pub fn unbind(&mut self, route_id: RouteId) {
        let removed = self.routes.swap_remove(&route_id);
        if let Some(route) = removed {
            if let Some(src) = route.source {
                self.try_remove(route_id, &Src(src));
            }
            if let Some(dst) = route.destination {
                self.try_remove(route_id, &Dst(dst));
            }
        }
    }

    fn try_remove(&mut self, route_id: RouteId, bin: &RouteBinding) {
        if let Some(bins) = self.bindings.get_mut(bin) {
            if let Some((idx, _)) = bins.iter().enumerate().find(|(i, v)| **v == route_id) {
                bins.swap_remove(idx);
            } else {
                rprintln!("Route id {} not found in bindings {:?} index: {:?}", route_id, bin, bins)
            }
        } else {
            rprintln!("Route has source {:?} but is bindings is empty", bin)
        }
    }
}

