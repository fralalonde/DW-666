use heapless::{IndexMap, FnvIndexMap, Vec, FnvIndexSet};
use enum_map::EnumMap;
use crate::midi::{Packet, Channel, SysexMatcher, U4, Cull, Filter};
use crate::midi::status::is_channel_status;
use self::RouteBinding::*;
use hash32;
use core::sync::atomic::AtomicU16;
use core::sync::atomic::Ordering::Relaxed;
use core::convert::TryFrom;
use core::iter::FromIterator;
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

    /// Return true if router should proceed sending packets in buffer
    /// Return false to discard any packets in buffer
    /// Does not affect other routes
    fn apply(&mut self, context: &mut RoutingContext) -> bool {
        for filter in &mut self.filters {
            if !filter.apply(context) {
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
#[derive(Copy, Clone, Debug,  Eq, PartialEq)]
pub enum RouteBinding {
    Src(Interface),
    Dst(Interface),
    Clock,
}

impl hash32::Hash for RouteBinding {
    fn hash<H: hash32::Hasher>(&self, state: &mut H) {
        match self {
            Src(idx) =>  {
                state.write(&[0x4]);
                idx.hash(state);
            }
            Dst(idx) =>  {
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
    /// Packets can be scheduled to be sent in the future
    Packet(Duration, Packet),
    /// Clock events are always "now"
    Clock,
}

pub struct ScheduledPacket(Instant, Packet);

pub struct RoutingContext {
    events: Vec<RouterEvent, 64>,
    destinations: FnvIndexSet<Interface, 4>,
}

impl RoutingContext {
    fn clear(&mut self) {
        self.events.clear();
        self.destinations.clear();
    }

    pub fn send_packet(&mut self, packet: Packet) {
        self.schedule_packet(0, packet)
    }

    pub fn schedule_packet(&mut self, delay: Duration, packet: Packet) {
        if let Err(e) = self.events.push(RouterEvent::Packet(0, packet)) {
            rprintln!("Dropped Packet: Routing buffer full")
        }
    }

    pub fn add_destination(&mut self, destination: Interface) {
        if let Err(e) = self.destinations.insert(destination) {
            rprintln!("Destination dropped: Routing buffer full")
        }
    }
}

type RouteVec = Vec<RouteId, 8>;

#[derive(Debug, Default)]
pub struct Router {
    triggers: FnvIndexMap<RouteBinding, RouteVec, 16>,
    routes: FnvIndexMap<RouteId, Route, 64>,
    // TODO route ID pooling instead
    next_route_id: AtomicU16,
    context: RoutingContext,
}

use crate::dispatch_from;
use rtic::cyccnt::Instant;

impl Router {
    pub fn dispatch_from(&mut self, now: Instant, packet: Packet, source: Interface, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.triggers.get(&Src(source)) {
            self.dispatch(now, RouterEvent::Packet(0, packet), route_ids, spawn, schedule)
        }
    }

    pub fn dispatch_to(&mut self, now: Instant, packet: Packet, destination: Interface, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.triggers.get(&Dst(destination)) {
            self.dispatch(now, RouterEvent::Packet(0, packet), route_ids, spawn, schedule)
        }
    }

    pub fn dispatch_clock(&mut self, now: Instant, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.triggers.get(&Clock) {
            self.dispatch(now, RouterEvent::Clock, route_ids, spawn, schedule)
        }
    }

    fn dispatch(&mut self, now: Instant, event: RouterEvent, route_ids: &RouteVec, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        for route_id in route_ids {
            if let Some(mut route) = self.routes.get(route_id) {
                self.context.clear();
                self.context.events.push(event);
                if let Some(dest) = route.destination {
                    self.context.destinations.insert(dest);
                }
                if route.apply(&mut self.context) {
                    for event in self.context.events {
                        if let RouterEvent::Packet(delay, packet) = event {
                            if delay == 0 {
                                spawn.send_midi(route.destination, packet).unwrap()
                            } else {
                                // quantized or delayed
                                schedule.send_midi(route.destination, packet).unwrap()
                            }
                        }
                    }
                }
            } else {
                rprintln!("Route ID {} triggered but not found", route_id)
            }
        }
    }

    pub fn bind(mut self, route: Route) -> RouteId {
        let route_id = self.next_route_id.fetch_add(1, Relaxed);

        if let Some(src) = route.source {
            self.bind_route(&Src(src), route_id);
        }

        if let Some(dst) = route.destination {
            self.bind_route(&Dst(dst), route_id);
        }

        for f in route.filters {
            for b in f.bindings() {
                self.bind_route(b, route_id);
            }
        }

        self.routes.insert(route_id, route);

        route_id
    }

    fn bind_route(&mut self, binding: &RouteBinding, route_id: RouteId) {
        // FIXME heapless fnvmap does not have entry() yet
        if let Some(route_ids) = self.triggers.get_mut(binding) {
            route_ids.push(route_id);
        } else {
            let mut route_ids = Vec::new();
            route_ids.push(route_id);
            self.sources.insert(*binding, route_ids);
        }
    }

    pub fn unbind(&mut self, route_id: RouteId) {
        if let Some(route) = self.routes.swap_remove(&route_id) {
            if let Some(src) = route.source {
                if let Some(sources) = self.sources.get_mut(&src) {
                    if let Some((idx, _)) = sources.iter().enumerate().find(|(i, v)| **v == route_id) {
                        sources.swap_remove(idx);
                    } else {
                        rprintln!("Route id {} not found in source {:?} index: {:?}", route_id, src, sources)
                    }
                } else {
                    rprintln!("Route has source {:?} but is index is empty", src)
                }
            }
            if let Some(dst) = route.destination {
                if let Some(destinations) = self.destinations.get_mut(&dst) {
                    if let Some((idx, _)) = destinations.iter().enumerate().find(|(i, v)| **v == route_id) {
                        destinations.swap_remove(idx);
                    } else {
                        rprintln!("Route id {} not found in source {:?} index: {:?}", route_id, dst, destinations)
                    }
                } else {
                    rprintln!("Route has source {:?} but is index is empty", dst)
                }
            }
        }
    }
}

