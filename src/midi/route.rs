use crate::midi::{Packet, Tag, Interface, MidiError};
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
    filters: Vec<Box<dyn FnMut(Packet, &mut RouteContext) -> bool + Send + 'static>>,
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
        where F: FnMut(Packet, &mut RouteContext) -> bool + Send + 'static
    {
        self.filters.push(Box::new(filter));
        self
    }

    /// Return true if router should forward event to destinations
    /// Return false to discard the event
    /// Does not affect other routes
    fn apply_filters(&mut self, packet: Packet) -> Option<RouteContext> {
        let mut context = RouteContext::default();
        for filter in &mut self.filters {
            if !(filter)(packet, &mut context) {
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

#[derive(Default)]
pub struct RouteContext {
    pub destinations: HashSet<Interface>,
    pub tags: HashMap<Tag, Vec<u8>>,
}

type RouteVec = Vec<Handle>;

#[derive(Default)]
pub struct Router {
    bindings: HashMap<RouteBinding, RouteVec>,
    virtuals: HashMap<u16, Box<dyn FnMut(Instant, Packet, RouteContext, dispatch_midi::Spawn) + 'static + Send>>,
    routes: HashMap<Handle, Route>,
    // TODO route ID pooling instead
}

use crate::{dispatch_midi, Handle, NEXT_HANDLE};
use rtic::cyccnt::{Instant};
use alloc::boxed::Box;
use core::fmt::{Debug};
use crate::time::Tasks;

impl Router {
    pub fn dispatch_midi(&mut self, now: Instant, packet: Packet, source: RouteBinding, spawn: dispatch_midi::Spawn) {
        if let Some(route_ids) = self.bindings.get(&source).cloned() {
            // routes are independent from each other, could be processed concurrently
            for route_id in route_ids {
                if let Err(err) = self.dispatch_route_id(route_id, now, packet, spawn) {
                    rprintln!("Route {} dispatch failed: {:?}", route_id, err)
                }
            }
        }
    }

    pub fn dispatch_route_id(&mut self, route_id: Handle, now: Instant, packet: Packet, spawn: dispatch_midi::Spawn) -> Result<(), MidiError> {
        if let Some(route) = self.routes.get_mut(&route_id) {
            if let Some(context) = route.apply_filters(packet) {
                match route.destination {
                    Some(Interface::Virtual(virt_id)) =>
                        if let Some(virt) = self.virtuals.get_mut(&virt_id) {
                            (virt)(now, packet, context, spawn)
                        }
                    Some(destination) => spawn.send_midi(destination, packet)?,
                    None => {}
                }
            }
        } else {
            rprintln!("Route ID {} triggered but not found", route_id)
        }
        Ok(())
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


    pub fn add_interface<F>(&mut self, fun: F) -> Handle
        where F: FnMut(Instant, Packet, RouteContext, dispatch_midi::Spawn) + 'static + Send
    {
        let virt_id = NEXT_HANDLE.fetch_add(1, Relaxed);
        self.virtuals.insert(virt_id, Box::new(fun));
        Interface::Virtual(virt_id);
        virt_id
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

