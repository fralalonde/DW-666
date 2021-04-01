use heapless::{IndexMap, FnvIndexMap, Vec};
use enum_map::EnumMap;
use crate::midi::{Packet, Channel};
use crate::midi::status::is_channel_status;
use self::Binding::*;
use hash32;
use core::sync::atomic::AtomicU16;
use core::sync::atomic::Ordering::Relaxed;

#[derive(Debug)]
pub enum Filter {
    FilterChannel(Channel),
    // TODO more transforms...
}

#[derive(Debug, Default)]
pub struct Route {
    priority: u8,
    source: Option<Interface>,
    destination: Option<Interface>,
    filters: Vec<Filter, 4>,
}

impl Route {
    
    /// Routes A -> B and B -> A
    pub fn circuit(interface1: Interface, interface2: Interface) -> (Self, Self) {
        (Self::link(interface1, interface2), Route::link(interface2, interface1))
    }

    /// Route A -> B
    pub fn link(interface1: Interface, interface2: Interface) -> Self {
        let mut route = Route::from(interface1);
        route.destination = Some(interface2);
        route
    }

    /// Route A -> A
    pub fn echo(interface: Interface) -> Self {
        let mut route = Route::default();
        route.source = Some(interface);
        route.destination = Some(interface);        
        route
    }

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

    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    fn route(&self, packet: Packet) -> Option<Packet> {
        let mut result = Some(packet);
        for n in &self.filters {
            match n {
                Filter::FilterChannel(only) => {
                    if let Some(channel) = packet.channel() {
                        if channel != *only {
                            return None;
                        }
                    }
                }
            }
        }
        result
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Interface {
    USB,
    Serial(u8),
}

impl hash32::Hash for Interface {
    fn hash<H: hash32::Hasher>(&self, state: &mut H) {
        match self {
            Interface::USB => state.write(&[0]),
            Interface::Serial(idx) => state.write(&[0xF + idx])
        }
    }
}


#[derive(Copy, Clone, Debug)]
pub enum Binding {
    Src(Interface),
    Dst(Interface),
    Any,
}

pub type RouteId = u16;

#[derive(Debug, Default)]
pub struct Router {
    sources: FnvIndexMap<Interface, Vec<RouteId, 8>, 16>,
    destinations: FnvIndexMap<Interface, Vec<RouteId, 8>, 16>,
    routes: FnvIndexMap<RouteId, Route, 64>,
    // TODO route ID pooling instead
    next_route_id: AtomicU16,
}

impl Router {
    pub fn dispatch(&self, bind: Binding, packet: Packet) {
        todo!()
    }

    pub fn install_route(mut self, route: Route) -> RouteId {
        let route_id = self.next_route_id.fetch_add(1, Relaxed);

        if let Some(src) = route.source {
            // FIXME heapless fnvmap does not have entry() yet
            if let Some(route_ids) = self.sources.get_mut(&src) {
                route_ids.push(route_id);
            } else {
                let mut route_ids = Vec::new();
                route_ids.push(route_id);
                self.sources.insert(src, route_ids);
            }
        }

        if let Some(dst) = route.destination {
            // FIXME heapless fnvmap does not have entry() yet
            if let Some(route_ids) = self.destinations.get_mut(&dst) {
                route_ids.push(route_id);
            } else {
                let mut route_ids = Vec::new();
                route_ids.push(route_id);
                self.destinations.insert(dst, route_ids);
            }
        }

        self.routes.insert(route_id, route);

        route_id
    }

    pub fn drop_route(&mut self, route_id: RouteId) {
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
        }
    }
}

