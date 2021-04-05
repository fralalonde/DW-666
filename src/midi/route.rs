use crate::midi::{Packet, Channel, SysexMatcher, U4, Cull, Filter};
use crate::midi::status::is_channel_status;
use self::RouteBinding::*;

// use alloc::collections::{
//     BTreeMap as HashMap,
//     BTreeSet as HashSet
// };
use alloc::vec::Vec;
use hashbrown::{HashMap, HashSet};

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
    filters: Vec<Filter>,
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Interface {
    USB,
    Serial(u8),
    // TODO virtual interfaces ?
}

/// Events that may trigger a route
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RouteBinding {
    Src(Interface),
    Dst(Interface),
    Clock,
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
    events: Vec<RouterEvent>,
    destinations: HashSet<Interface>,
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
        self.events.push(RouterEvent::Packet(delay, packet))
    }

    pub fn add_destination(&mut self, destination: Interface) {
        self.destinations.insert(destination);
    }
}

type RouteVec = Vec<RouteId>;

#[derive(Debug)]
pub struct Router {
    bindings: HashMap<RouteBinding, RouteVec>,
    routes: HashMap<RouteId, Route>,
    // TODO route ID pooling instead
    next_route_id: AtomicU16,
    context: RoutingContext,
}

use crate::dispatch_from;
use rtic::cyccnt::Instant;
use core::cell::RefCell;
use cortex_m::asm::delay;

impl Router {

    pub fn new() -> Self {
        Router {
            bindings: HashMap::new(),
            routes: HashMap::new(),
            next_route_id: Default::default(),
            context: Default::default()
        }
    }

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
        let context = &mut self.context;
        for route_id in route_ids {
            if let Some(mut route) = routes.get_mut(route_id) {
                context.clear();
                context.events.push(event);
                if let Some(dest) = route.destination {
                    context.destinations.insert(dest);
                }
                if route.apply(context) {
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
        rprintln!("binding route {:?}", route);
        delay(500_000);

        let route_id = self.next_route_id.fetch_add(1, Relaxed);
        rprintln!("route id {}", route_id);

        if let Some(src) = route.source {
            rprintln!("src {:?}", src);
            delay(500_000);
            self.bind_route(&Src(src), route_id);
        }

        if let Some(dst) = route.destination {
            rprintln!("dst {:?}", dst);
            delay(500_000);
            self.bind_route(&Dst(dst), route_id);
        }

        for f in &route.filters {
            for b in f.bindings() {
                rprintln!("filter {:?}", b);
                delay(500_000);
                self.bind_route(b, route_id);
            }
        }

        rprintln!("insert {:?}", route);
        delay(500_000);
        self.routes.insert(route_id, route);

        route_id
    }

    fn bind_route(&mut self, binding: &RouteBinding, route_id: RouteId) {
        self.bindings.entry(*binding).or_insert_with(|| Vec::new()).push(route_id);
    }

    pub fn unbind(&mut self, route_id: RouteId) {
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

