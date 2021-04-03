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
    /// Packets can be scheduled to be sent in the future
    Packet(Duration, Packet),
    /// Clock events are always "now"
    Clock,
}

pub struct ScheduledPacket(Instant, Packet);

#[derive(Debug, Default)]
pub struct RoutingContext {
    events: Vec<RouterEvent, 64>,
    destinations: FnvIndexSet<Interface, 4>,
}

impl RoutingContext {
    fn clear(&mut self) {
        self.events.clear();
        self.destinations.clear();
    }

    pub fn events(&self) -> &[RouterEvent] {
        &self.events
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
    bindings: FnvIndexMap<RouteBinding, RouteVec, 16>,
    routes: RefCell<FnvIndexMap<RouteId, Route, 64>>,
    // TODO route ID pooling instead
    next_route_id: AtomicU16,
    context: RefCell<RoutingContext>,
}

use crate::dispatch_from;
use rtic::cyccnt::Instant;
use core::cell::RefCell;

impl Router {
    pub fn dispatch_from(&self, now: Instant, packet: Packet, source: Interface, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.bindings.get(&Src(source)) {
            self.dispatch(now, RouterEvent::Packet(0, packet), route_ids, spawn, schedule)
        }
    }

    pub fn dispatch_to(&self, now: Instant, packet: Packet, destination: Interface, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.bindings.get(&Dst(destination)) {
            self.dispatch(now, RouterEvent::Packet(0, packet), route_ids, spawn, schedule)
        }
    }

    pub fn dispatch_clock(&self, now: Instant, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        if let Some(route_ids) = self.bindings.get(&Clock) {
            self.dispatch(now, RouterEvent::Clock, route_ids, spawn, schedule)
        }
    }

    fn dispatch(&self, now: Instant, event: RouterEvent, route_ids: &RouteVec, spawn: dispatch_from::Spawn, schedule: dispatch_from::Schedule) {
        let mut routes = self.routes.borrow_mut();
        let mut context = self.context.borrow_mut();
        for route_id in route_ids {
            if let Some(mut route) = routes.get_mut(route_id) {
                context.clear();
                context.events.push(event);
                if let Some(dest) = route.destination {
                    context.destinations.insert(dest);
                }
                if route.apply(&mut context) {
                    for event in &context.events {
                        if let Some(destination) = route.destination {
                            if let RouterEvent::Packet(delay, packet) = event {
                                if *delay == 0 {
                                    spawn.send_midi(destination, *packet).unwrap()
                                } else {
                                    // quantized or delayed
                                    schedule.send_midi(Instant::now(), destination, *packet).unwrap()
                                }
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

        self.routes.borrow_mut().insert(route_id, route);

        route_id
    }

    fn bind_route(&mut self, binding: &RouteBinding, route_id: RouteId) {
        // FIXME heapless fnvmap does not have entry() yet
        if let Some(route_ids) = self.bindings.get_mut(binding) {
            route_ids.push(route_id);
        } else {
            let mut route_ids = Vec::new();
            route_ids.push(route_id);
            self.bindings.insert(*binding, route_ids);
        }
    }

    pub fn unbind(&mut self, route_id: RouteId) {
        let removed = self.routes.borrow_mut().swap_remove(&route_id);
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

