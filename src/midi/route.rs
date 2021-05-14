use crate::midi::{Packet, Tag, Interface};
use self::RouteBinding::*;

use alloc::vec::Vec;
use hashbrown::{HashMap, HashSet};

use core::sync::atomic::Ordering::Relaxed;

pub trait Service {
    fn start(&mut self, now: rtic::cyccnt::Instant, router: &mut Router, tasks: &mut Tasks);
    fn stop(&mut self, _router: &mut Router) {}
}

#[derive(Default)]
pub struct Route {
    source: Option<Interface>,
    destination: Option<Interface>,
    filters: Vec<Box<dyn FnMut(RouterEvent, &mut RouteContext) -> bool + Send + 'static>>,
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
        where F: FnMut(RouterEvent, &mut RouteContext) -> bool + Send + 'static
    {
        self.filters.push(Box::new(filter));
        self
    }

    /// Return true if router should forward event to destinations
    /// Return false to discard the event
    /// Does not affect other routes
    fn apply(&mut self, event: RouterEvent) -> Option<RouteContext> {
        let mut context = RouteContext::default();
        for filter in &mut self.filters {
            if !(filter)(event, &mut context) {
                return None;
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


#[derive(Debug, Copy, Clone)]
pub enum RouterEvent {
    /// Original packet gets time "now" by default
    /// Packets can be scheduled to be sent in the future with Duration > 0
    Packet(Packet),
    /// Clock events are always "now"
    Clock,
}

#[derive(Debug, Default)]
pub struct RouteContext {
    pub destinations: HashSet<Interface>,
    pub tags: HashMap<Tag, Vec<u8>>,
}

pub struct Handler {
    inner: Box<dyn FnMut(Instant, RouterEvent, RouteContext, dispatch_from::Spawn) + 'static + Send>,
}

impl Debug for Handler {
    fn fmt(&self, _f: &mut Formatter<'_>) -> core::fmt::Result {
        todo!()
    }
}

impl Handler {
    pub fn new<F>(fun: F) -> Self
        where F: FnMut(Instant, RouterEvent, RouteContext, dispatch_from::Spawn) + 'static + Send
    {
        Handler {
            inner: Box::new(fun)
        }
    }

    pub fn handle(&mut self, now: Instant, event: RouterEvent, ctx: RouteContext, spawn: dispatch_from::Spawn) {
        (self.inner)(now, event, ctx, spawn);
    }
}

type RouteVec = Vec<Handle>;

// pub trait Virtual2: Debug + Send {
//     fn apply(&mut self, now: Instant, event: RouterEvent, ctx: RouteContext, router: &mut Router, spawn: dispatch_from::Spawn);
// }

#[derive(Default)]
pub struct Router {
    bindings: HashMap<RouteBinding, RouteVec>,
    virtuals: HashMap<u16, Handler>,
    routes: HashMap<Handle, Route>,
    // TODO route ID pooling instead
}

use crate::{dispatch_from, Handle, NEXT_HANDLE};
use rtic::cyccnt::{Instant};
use alloc::boxed::Box;
use core::fmt::{Debug, Formatter};
use crate::time::Tasks;

impl Router {
    pub fn dispatch_from(&mut self, now: Instant, packet: Packet, source: Interface, spawn: dispatch_from::Spawn) {
        if let Some(route_ids) = self.bindings.get(&Src(source)).cloned() {
            self.dispatch(now, RouterEvent::Packet(packet), &route_ids, spawn)
        }
    }

    pub fn dispatch_to(&mut self, now: Instant, packet: Packet, destination: Interface, spawn: dispatch_from::Spawn) {
        if let Some(route_ids) = self.bindings.get(&Dst(destination)).cloned() {
            self.dispatch(now, RouterEvent::Packet(packet), &route_ids, spawn)
        }
    }

    pub fn dispatch_clock(&mut self, now: Instant, spawn: dispatch_from::Spawn) {
        if let Some(route_ids) = self.bindings.get(&Clock).cloned() {
            self.dispatch(now, RouterEvent::Clock, &route_ids, spawn)
        }
    }

    fn dispatch(&mut self, now: Instant, event: RouterEvent, route_ids: &RouteVec, spawn: dispatch_from::Spawn) {
        // routes are independent from each other, could be processed concurrently
        for route_id in route_ids {
            self.dispatch_route_id(*route_id, now, event, spawn)
        }
    }

    pub fn dispatch_route_id(&mut self, route_id: Handle, now: Instant, event: RouterEvent, spawn: dispatch_from::Spawn) {
        if let Some(route) = self.routes.get_mut(&route_id) {
            if let Some(context) = route.apply(event) {
                match route.destination {
                    Some(Interface::Virtual(virt_id)) =>
                        if let Some(virt) = self.virtuals.get_mut(&virt_id) {
                            virt.handle(now, event, context, spawn)
                        }
                    Some(destination) =>
                        if let RouterEvent::Packet(packet) = event {
                            spawn.send_midi(destination, packet).unwrap()
                        }
                    None => {}
                }
            }
        } else {
            rprintln!("Route ID {} triggered but not found", route_id)
        }
    }

    pub fn bind(&mut self, route: Route) -> Handle {
        let route_id = NEXT_HANDLE.fetch_add(1, Relaxed);

        if let Some(src) = route.source {
            self.bind_route(&Src(src), route_id);
        }

        if let Some(dst) = route.destination {
            self.bind_route(&Dst(dst), route_id);
        }

        self.routes.insert(route_id, route);
        route_id
    }


    pub fn add_interface(&mut self, handler: Handler) -> Interface {
        let virt_id = NEXT_HANDLE.fetch_add(1, Relaxed);
        self.virtuals.insert(virt_id, handler);
        Interface::Virtual(virt_id)
    }

    fn bind_route(&mut self, binding: &RouteBinding, route_id: Handle) {
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

