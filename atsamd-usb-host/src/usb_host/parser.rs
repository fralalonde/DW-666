use core::convert::TryFrom;

use utf16string::{LE, WStr};
use crate::DeviceDescriptor;
use crate::usb_host::{ConfigurationDescriptor, DescriptorType, EndpointDescriptor, InterfaceDescriptor};

pub enum DescriptorRef<'a> {
    Device(&'a DeviceDescriptor),
    Configuration(&'a ConfigurationDescriptor),
    Interface(&'a InterfaceDescriptor),
    Endpoint(&'a EndpointDescriptor),
    String(&'a WStr<LE>),
    Other(&'a [u8]),
}

pub struct DescriptorParser<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for DescriptorParser<'a> {
    type Item = DescriptorRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.buf.len() {
            return None;
        }

        let desc_len = self.buf[self.pos] as usize;
        let desc_type: u8 = self.buf[self.pos + 1];
        let desc_offset = unsafe { self.buf.as_ptr().add(self.pos as usize) };
        let body_offset = unsafe { self.buf.as_ptr().add((self.pos + 2) as usize) };
        let desc_next = self.pos + desc_len;

        let desc_ref = match DescriptorType::try_from(desc_type) {
            Ok(DescriptorType::Device) => Some(DescriptorRef::Device(unsafe { &*(desc_offset as *const _) })),
            Ok(DescriptorType::Configuration) => Some(DescriptorRef::Configuration(unsafe { &*(desc_offset as *const _) })),
            Ok(DescriptorType::Interface) => Some(DescriptorRef::Interface(unsafe { &*(desc_offset as *const _) })),
            Ok(DescriptorType::Endpoint) => Some(DescriptorRef::Endpoint(unsafe { &*(desc_offset as *const _) })),
            Ok(DescriptorType::String) => Some(DescriptorRef::String(unsafe { WStr::from_utf16le_unchecked(core::slice::from_raw_parts(body_offset as *const _, (desc_len - 2) as usize)) })),
            Err(_) => Some(DescriptorRef::Other(&self.buf[self.pos..desc_next])),
            _ => Some(DescriptorRef::Other(&self.buf[self.pos..desc_next])),
        };

        // move to next element
        self.pos = desc_next;
        desc_ref
    }
}

impl<'a> DescriptorParser<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn rewind(&mut self) {
        self.pos = 0;
    }
}