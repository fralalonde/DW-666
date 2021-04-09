use crate::midi::{Packet, Message, U7, Saturate};
use heapless::Vec;

use crate::midi::message::Message::{SysexEnd2, SysexEnd1, SysexEnd, SysexBegin, SysexCont, SysexEmpty, SysexSingleByte};

const MAX_PATTERN_TOKENS: usize = 8;
const MAX_CAPTURE_TOKENS: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct Range {
    min: u8,
    max: u8,
}

impl Range {
    fn contains(&self, byte: u8) -> bool {
        byte >= self.min && byte < self.max
    }
}

pub fn range(min: u8, max: u8) -> Range {
    assert!(min < max, "Invalid sysex value range: min ({}) is bigger than max ({})", min, max);
    Range {
        min,
        max,
    }
}

// pub struct SysexListenerHandle {}
//
// pub trait SysexDispatch {
//     fn listen(interface: Interface, matcher: Pattern) -> SysexListenerHandle;
//     fn unlisten(handle: SysexListenerHandle);
//     fn send(spawn: crate::dispatch_from::Spawn, interface: Interface, packets: impl IntoIterator<Item=Packet>);
// }

use num_enum::IntoPrimitive;
use core::convert::TryFrom;

#[derive(Debug, Clone, Copy, Eq, PartialEq, IntoPrimitive)]
#[repr(u8)]
pub enum Tag {
    Channel,
    Velocity,
    DeviceId,
    // Indexed parameters (Steps in sequence, generic knob / pad ID)
    Index,
    // Parameter code
    Param,
    // Value of parameter
    Value,
}

impl hash32::Hash for Tag {
    fn hash<H: hash32::Hasher>(&self, state: &mut H) {
        let b: u8 = (*self).into();
        state.write(&[b])
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SysexFragment {
    Slice(&'static [u8]),
    Byte(u8),
}

#[derive(Debug, Copy, Clone)]
pub enum SysexToken {
    Match(&'static [u8]),
    Ignore(u8),
    Val(u8),
    // Unconditional capture (values, etc)
    Capture(Tag),
    // Range capture
    CaptureRange(Tag, Range),
}

/// Used to send sysex
#[derive(Debug)]
pub struct Sequence {
    tokens: Vec<SysexFragment, MAX_PATTERN_TOKENS>,
    // current token to produce from
    tok_idx: usize,
    // current index inside token
    byte_idx: usize,
    total_bytes: usize,
}

impl Sequence {
    pub fn new(tokens: &[SysexFragment]) -> Self {
        Sequence {
            tokens: Vec::from_slice(tokens).unwrap(),
            tok_idx: 0,
            byte_idx: 0,
            total_bytes: 0,
        }
    }
}

impl Iterator for Sequence {
    type Item = Packet;

    fn next(&mut self) -> Option<Self::Item> {
        if self.tok_idx > self.tokens.len() {
            return None;
        }
        let mut bytes: Vec<u8, 3> = Vec::new();
        if self.tok_idx == self.tokens.len() {
            // mark as definitely done
            self.tok_idx += 1;
        } else {
            while bytes.len() < 3 {
                if self.tok_idx >= self.tokens.len() {
                    break;
                }
                let token = self.tokens[self.tok_idx];
                let tok_len = match token {
                    SysexFragment::Slice(slice) => {
                        bytes.push(slice[self.byte_idx]).unwrap();
                        slice.len()
                    }
                    SysexFragment::Byte(val) => {
                        bytes.push(val).unwrap();
                        1
                    }
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
pub struct Matcher {
    pattern: Vec<SysexToken, MAX_PATTERN_TOKENS>,
    matching: bool,
    // current token to produce from
    tok_idx: usize,
    // current index inside token
    byte_idx: usize,
    captured: Vec<(Tag, U7), MAX_CAPTURE_TOKENS>,
}

impl Matcher {
    pub fn new(tokens: &[SysexToken]) -> Self {
        Matcher {
            pattern: Vec::from_slice(tokens).unwrap(),
            matching: false,
            tok_idx: 0,
            byte_idx: 0,
            captured: Vec::new(),
        }
    }

    pub fn match_packet(&mut self, packet: Packet) -> Option<Vec<(Tag, U7), MAX_CAPTURE_TOKENS>> {
        if let Ok(message) = Message::try_from(packet) {
            let mut sysex_end = true;
            match message  {
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
                return Some(self.captured.clone())
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
        match &self.pattern[self.tok_idx] {
            SysexToken::Match(token) => {
                if token[self.byte_idx] != byte {
                    return self.fail_match();
                }
                tok_len = token.len()
            }
            SysexToken::Ignore(len) => {
                tok_len = *len as usize
            }
            SysexToken::Val(token) => {
                if *token != byte {
                    return self.fail_match();
                }
            }
            SysexToken::Capture(ttype) => {
                self.captured.push((*ttype, U7::saturate(byte))).unwrap();
            }
            SysexToken::CaptureRange(ttype, range) => {
                if range.contains(byte) {
                    self.captured.push((*ttype, U7::saturate(byte))).unwrap();
                } else {
                    return self.fail_match();
                }
            }
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
