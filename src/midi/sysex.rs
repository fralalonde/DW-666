use crate::midi::{Packet, Message};
use alloc::vec::Vec;

use crate::midi::message::Message::{SysexEnd2, SysexEnd1, SysexEnd, SysexBegin, SysexCont, SysexEmpty, SysexSingleByte};

// #[derive(Debug, Clone, Copy)]
// pub struct Range {
//     min: u8,
//     max: u8,
// }
//
// impl Range {
//     fn contains(&self, byte: u8) -> bool {
//         byte >= self.min && byte < self.max
//     }
// }
//
// pub fn range(min: u8, max: u8) -> Range {
//     assert!(min < max, "Invalid sysex value range: min ({}) is bigger than max ({})", min, max);
//     Range {
//         min,
//         max,
//     }
// }

// #[derive(Debug, Clone)]
// pub struct Buffer<T> {
//     inner: Box<[T]>,
//     len: usize,
// }
//
// impl<T> Deref for Buffer<T> {
//     type Target = [T];
//
//     fn deref(&self) -> &Self::Target {
//         self.inner.as_ref()
//     }
// }
//
// impl<T> Buffer<T> {
//     pub fn with_capacity(cap: usize) -> Self {
//         Buffer {
//             inner: unsafe { Box::new_uninit_slice(cap).assume_init() },
//             len: 0,
//         }
//     }
//
//     pub fn capacity(&self) -> usize {
//         self.inner.len()
//     }
//
//     pub fn push(&mut self, item: T) -> Result<(), MidiError> {
//         if self.len < self.capacity() {
//             self.inner[self.len] = item;
//             Ok(())
//         } else {
//             Err(MidiError::BufferFull)
//         }
//     }
//
//     pub fn clear(&mut self) {
//         self.len = 0
//     }
// }

use core::convert::TryFrom;
use hashbrown::HashMap;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Tag {
    Channel,
    Velocity,
    DeviceId,
    // Indexed parameters (Steps in sequence, generic knob / pad ID)
    Index,
    // Parameter code
    ParamId,
    // Value of parameter
    ValueU7,
    // Value of parameter
    MsbValueU4,
    // Value of parameter
    LsbValueU4,
    // Raw data
    Dump(usize),
}

impl Tag {
    pub fn size(&self) -> usize {
        match self {
            Tag::Dump(len) => *len,
            _ => 1,
        }
    }
}

/// Used to send sysex
/// Accepts same Token as matcher for convenience, but only Match and Val value are sent
// #[derive(Debug)]
pub struct RequestSequence {
    tokens: Vec<ResponseToken>,
    // current token to produce from
    tok_idx: usize,
    // current index inside token
    byte_idx: usize,
    total_bytes: usize,
}

impl RequestSequence {
    pub fn new(tokens: Vec<ResponseToken>) -> Self {
        RequestSequence {
            tokens,
            tok_idx: 0,
            byte_idx: 0,
            total_bytes: 0,
        }
    }
}

impl Iterator for RequestSequence {
    type Item = Packet;

    fn next(&mut self) -> Option<Self::Item> {
        if self.tok_idx > self.tokens.len() {
            return None;
        }
        let mut bytes: Vec<u8> = Vec::new();
        if self.tok_idx == self.tokens.len() {
            // mark as definitely done
            self.tok_idx += 1;
        } else {
            while bytes.len() < 3 {
                if self.tok_idx >= self.tokens.len() {
                    break;
                }
                let token = &self.tokens[self.tok_idx];
                let tok_len = match token {
                    ResponseToken::Seq(slice) => {
                        bytes.push(slice[self.byte_idx]);
                        slice.len()
                    }
                    ResponseToken::Val(val) => {
                        bytes.push(*val);
                        1
                    }
                    _ => 0
                };
                self.byte_idx += 1;
                if self.byte_idx >= tok_len {
                    // move on to next token
                    self.tok_idx += 1;
                    self.byte_idx = 0;
                }
            }
        }
        self.total_bytes += bytes.len();
        let done = self.tok_idx >= self.tokens.len();
        Some(Packet::from(
            match (bytes.len(), done, self.total_bytes) {
                (2, false, _) => SysexBegin(bytes[0], bytes[1]),
                (3, false, _) => SysexCont(bytes[0], bytes[1], bytes[2]),

                // sysex start + end ("special cases")
                (0, true, 0) => SysexEmpty,
                (1, true, 1) => SysexSingleByte(bytes[0]),

                // sysex end
                (0, true, _) => SysexEnd,
                (1, true, _) => SysexEnd1(bytes[0]),
                (2, true, _) => SysexEnd2(bytes[0], bytes[1]),

                (p_len, done, t_len) => {
                    rprintln!("Could not build sysex packet: p_len({}) done({}) t_len({})", p_len, done, t_len);
                    return None;
                }
            }
        ))
    }
}

#[derive(Debug)]
pub enum ResponseToken {
    Seq(&'static [u8]),
    Buf(Vec<u8>),
    Skip(usize),
    Val(u8),
    Cap(Tag),
}

pub type CaptureBuffer = HashMap<Tag, Vec<u8>>;

#[derive(Debug)]
pub struct ResponseMatcher {
    pattern: Vec<ResponseToken>,
    matching: bool,
    // current token to produce from
    tok_idx: usize,
    // current index inside token
    byte_idx: usize,
    captured: CaptureBuffer,
}

impl ResponseMatcher {
    pub fn new(pattern: Vec<ResponseToken>) -> Self {
        ResponseMatcher {
            pattern,
            matching: false,
            tok_idx: 0,
            byte_idx: 0,
            captured: CaptureBuffer::default(),
        }
    }

    pub fn match_packet(&mut self, packet: Packet) -> Option<CaptureBuffer> {
        if let Ok(message) = Message::try_from(packet) {
            let mut sysex_end = true;
            match message {
                SysexBegin(byte0, byte1) => {
                    self.begin_match();
                    self.matching = self.advance(byte0) && self.advance(byte1);
                    sysex_end = false;
                }
                SysexSingleByte(byte0) => {
                    self.begin_match();
                    self.matching = self.advance(byte0);
                }
                SysexEmpty => {
                    self.begin_match();
                    self.matching = true;
                }
                SysexCont(byte0, byte1, byte2) => {
                    self.matching &= self.advance(byte0) && self.advance(byte1) && self.advance(byte2);
                    sysex_end = false;
                }
                SysexEnd => {}
                SysexEnd1(byte0) => self.matching &= self.advance(byte0),
                SysexEnd2(byte0, byte1) => self.matching &= self.advance(byte0) && self.advance(byte1),
                _ => self.matching = false,
            }

            if self.matching & sysex_end {
                self.matching = false;
                return Some(self.captured.clone());
            }
        }
        None
    }

    fn begin_match(&mut self) {
        self.tok_idx = 0;
        self.byte_idx = 0;
        self.captured.clear();
    }

    /// Returns true if byte matched the pattern or was captured
    /// Returns false if byte diverges from pattern
    /// Once this method returns false, every subsequent invocation will also return false until a new Sysex message starts
    fn advance(&mut self, byte: u8) -> bool {
        // fast exit if match previously failed
        if self.tok_idx >= self.pattern.len() {
            return false;
        }
        let mut tok_len = 1;
        match &mut self.pattern[self.tok_idx] {
            ResponseToken::Seq(token) => {
                if token[self.byte_idx] != byte {
                    return self.fail_match();
                }
                tok_len = token.len()
            }
            ResponseToken::Skip(len) => {
                tok_len = *len as usize
            }
            ResponseToken::Val(token) => {
                if *token != byte {
                    return self.fail_match();
                }
            }
            ResponseToken::Cap(tag) => {
                self.captured.entry(*tag)
                    .or_insert_with(|| Vec::with_capacity(tag.size()))
                    .push(byte);
                tok_len = tag.size()
            }
            ResponseToken::Buf(_) => {}
        };
        self.byte_idx += 1;
        if self.byte_idx >= tok_len {
            // move on to next token
            self.tok_idx += 1;
            self.byte_idx = 0;
        }
        true
    }

    #[inline]
    fn fail_match(&mut self) -> bool {
        self.tok_idx = self.pattern.len();
        false
    }
}
