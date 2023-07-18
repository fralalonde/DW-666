use core::fmt::{Debug};

use hash32::{Hasher};
use heapless::{FnvIndexMap, Vec};
use heapless::spsc::Queue;
use spin::mutex::SpinMutex;
use crate::{MidiError, Packet};

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PortId {
    Usb(usize),
    Serial(u8),
}

#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PortDirection {
    // Packets coming in from other devices
    In,
    // Packets going out to other devices
    Out,
}

impl hash32::Hash for PortId {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        match self {
            PortId::Usb(id) => {
                state.write(&[1]);
                state.write(&id.to_le_bytes())
            }
            PortId::Serial(id) => {
                state.write(&[2, *id]);
            }
        }
    }
}

#[derive(Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PortInfo {
    pub port_id: PortId,
    pub direction: PortDirection,
    // TODO figure out strings
    // name: &'str
}

pub type PortHandle = usize;

pub struct MidiPort {
    info: PortInfo,
    buffer: Queue<Packet, MAX_BUFFERED_PACKETS>,
}

const MAX_BUFFERED_PACKETS: usize = 16;
const MAX_PORTS: usize = 16;

pub trait MidiPorts {
    /// Take a port from the pool
    fn acquire_port(&self, info: PortInfo) -> Result<PortHandle, MidiError>;

    /// Put port back in pool
    fn release_port(&self, handle: &PortHandle);

    /// Try to read a packet from the port
    fn read(&self, handle: &PortHandle) -> Result<Option<Packet>, MidiError>;

    /// Write a packet to a port
    fn write(&self, handle: &PortHandle, packet: Packet) -> Result<(), MidiError>;

    /// Enumerate existing port handles
    fn list_ports(&self) -> Vec<PortHandle, MAX_PORTS>;

    fn space(&self, handle: &PortHandle) -> Result<usize, MidiError>;

    fn info(&self, handle: &PortHandle) -> Result<PortInfo, MidiError>;
}

// pub type MidiOutFn = &'static mut (dyn FnMut(Packet) -> bool + Send + Sync);
// pub type MidiInFn = &'static mut (dyn FnMut() -> Option<Packet> + Send + Sync);

pub struct MidiRegistry<const N: usize> {
    inner: SpinMutex<MidiRegistryInner<N>>,
}

impl<const N: usize> MidiRegistry<N> {
    fn with_port<R, F: Fn(&mut MidiPort) -> Result<R, MidiError>>(&self, handle: &PortHandle, fun: F) -> Result<R, MidiError> {
        if let Some(port) = self.inner.lock().ports.get_mut(handle) {
           fun(port)
        } else {
            Err(MidiError::InvalidPort)
        }
    }
}

impl<const N: usize> MidiPorts for MidiRegistry<N> {
    fn list_ports(&self) -> Vec<PortHandle, MAX_PORTS> {
        self.inner.lock().list_ports()
    }

    /// Take a port from the pool
    fn acquire_port(&self, info: PortInfo) -> Result<PortHandle, MidiError> {
        self.inner.lock().acquire_port(info)
    }

    /// Put port back in pool
    fn release_port(&self, handle: &PortHandle) {
        self.inner.lock().release_port(handle)
    }

    /// Try to read a packet from the port
    fn read(&self, handle: &PortHandle) -> Result<Option<Packet>, MidiError> {
        self.with_port(handle, |port| Ok(port.buffer.dequeue()))
    }

    /// Write a packet to a port
    fn write(&self, handle: &PortHandle, packet: Packet) -> Result<(), MidiError> {
        self.with_port(handle, |port| port.buffer.enqueue(packet).or(Err(MidiError::BufferFull)))
    }

    fn space(&self, handle: &PortHandle) -> Result<usize, MidiError> {
        self.with_port(handle, |port| Ok(port.buffer.capacity() - port.buffer.len()))
    }

    fn info(&self, handle: &PortHandle) -> Result<PortInfo, MidiError> {
        self.with_port(handle, |port| Ok(port.info))
    }
}

pub struct MidiRegistryInner<const N: usize> {
    next_port_handle: usize,
    ports: FnvIndexMap<PortHandle, MidiPort, MAX_PORTS>,
}

impl<const N: usize> MidiRegistryInner<N> {
    fn list_ports(&self) -> Vec<PortHandle, MAX_PORTS> {
        // FIXME find a way to just collect() keys?
        let mut ids = Vec::new();
        for p in self.ports.keys() {
            let _ = ids.push(*p);
        }
        ids
    }

    /// Take a port from the pool
    fn acquire_port(&mut self, info: PortInfo) -> Result<PortHandle, MidiError> {
        if self.ports.len() == self.ports.capacity() {
            return Err(MidiError::TooManyPorts);
        }

        let new_handle = self.next_port_handle;
        self.next_port_handle += 1;
        let new_port = MidiPort {
            info,
            buffer: Default::default(),
        };
        let _ = self.ports.insert(new_handle, new_port);
        Ok(new_handle)
    }

    /// Put port back in pool
    fn release_port(&mut self, handle: &PortHandle) {
        let removed = self.ports.remove(handle).is_some();
        debug_assert!(removed)
    }

}
