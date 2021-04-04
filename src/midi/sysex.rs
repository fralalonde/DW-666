use crate::midi::{Packet, Interface};
use heapless::Vec;

use core::ops::{Deref};
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

pub struct SysexListenerHandle {}

pub trait SysexDispatch {
    fn listen(interface: Interface, matcher: SysexMatcher) -> SysexListenerHandle;
    fn unlisten(handle: SysexListenerHandle);
    fn send(spawn: crate::dispatch_from::Spawn, interface: Interface, packets: impl IntoIterator<Item=Packet>);
}

#[derive(Debug, Clone, Copy)]
pub enum VarType {
    // Sysex Device ID
    Device,
    // Indexed parameters (Steps in sequence, generic knob / pad ID)
    Index,
    // Parameter code
    Param,
    // Value of parameter
    Value,
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
    Capture(VarType),
    // Range capture
    CaptureRange(VarType, Range),
}

/// Used to send sysex
#[derive(Debug)]
pub struct SysexPackets {
    tokens: Vec<SysexFragment, MAX_PATTERN_TOKENS>,
    // current token to produce from
    tok_idx: usize,
    // current index inside token
    byte_idx: usize,
    total_bytes: usize,
}

impl SysexPackets {
    pub fn sequence(tokens: &[SysexFragment]) -> Self {
        SysexPackets {
            tokens: Vec::from_slice(tokens).unwrap(),
            tok_idx: 0,
            byte_idx: 0,
            total_bytes: 0,
        }
    }

    pub fn and_then(mut self, tokens: &[SysexFragment]) -> Self {
        self.tokens.extend_from_slice(tokens).unwrap();
        self
    }
}

impl Iterator for SysexPackets {
    type Item = Packet;

    fn next(&mut self) -> Option<Self::Item> {
        if self.tok_idx > self.tokens.len() {
            return None
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
                    return None
                }
            }
        ))
    }
}

#[derive(Debug)]
pub struct SysexMatcher {
    tokens: Vec<SysexToken, MAX_PATTERN_TOKENS>,
}

impl Deref for SysexMatcher {
    type Target = [SysexToken];

    fn deref(&self) -> &Self::Target {
        self.tokens.as_slice()
    }
}

impl SysexMatcher {
    pub fn pattern(tokens: &[SysexToken]) -> Self {
        SysexMatcher {
            tokens: Vec::from_slice(tokens).unwrap(),
        }
    }

    pub fn matcher(&self) -> Matcher {
        Matcher {
            pattern: &self,
            tok_idx: 0,
            byte_idx: 0,
            captured: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Matcher<'a> {
    pattern: &'a SysexMatcher,
    // current token to produce from
    tok_idx: usize,
    // current index inside token
    byte_idx: usize,
    captured: Vec<(VarType, u8), MAX_CAPTURE_TOKENS>,
}

impl<'a> Matcher<'a> {
    /// Returns true if new byte matches expected byte value or range, or if byte is captured
    /// Returns false if new byte does falls outside expected pattern
    /// Once this method returns false, every subsequent invocation will also return false
    pub fn advance(&mut self, byte: u8) -> bool {
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
                self.captured.push((*ttype, byte)).unwrap();
            }
            SysexToken::CaptureRange(ttype, range) => {
                if range.contains(byte) {
                    self.captured.push((*ttype, byte)).unwrap();
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
