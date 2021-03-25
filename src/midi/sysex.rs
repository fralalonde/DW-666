use crate::midi::{Packet, CodeIndexNumber, MidiError};
use crate::midi::status::{is_non_status, SYSEX_END, SYSEX_START};
use crate::midi::packet::CodeIndexNumber::SystemCommonLen1;
use CodeIndexNumber::{SysexEndsNext2, SysexEndsNext3, Sysex};
use heapless::Vec;
use const_arrayvec::ArrayVec;
use crate::midi::sysex::MatcherState::{Init, PartialMatch, FullMatch};

struct SysexBuffer<const N: usize> {
    inner: ArrayVec<u8, N>
}

impl<const N: usize> SysexBuffer<N> {
    pub fn new() -> Self {
        SysexBuffer {
            inner: ArrayVec::new(),
        }
    }

    /// Returns the number of bytes added
    pub fn append_from(&mut self, packet: Packet) -> Result<u8, MidiError> {
        let body = packet.sysex_body();
        if self.inner.len() < body.len() {
            Err(MidiError::SysexBufferFull)?
        }
        self.inner[..body.len()].copy_from_slice(body);
        Ok(body.len() as u8)
    }
}

pub enum ByteMask {
    ExactMatch,
    Ignore,
    // Capture,
}

pub enum MatcherState {
    // Until first bytes are received
    Init,

    // Partial match if all bytes received matched but pattern not yet complete
    PartialMatch,

    // Full match if all bytes matched and complete pattern was covered
    FullMatch,

    // Matcher
    Negative,
}

// #[derive(Copy, Clone)]
// pub struct SysexMatcher<const N: usize> {
//     pattern: ArrayVec<u8, N>,
//     state: MatcherState,
//     position: usize,
//     cap_len: usize,
// }
//
// impl<const N: usize> SysexMatcher<N> {
//     pub fn pattern(pattern: &[u8], cap_len: usize) -> Self {
//         let mut p = ArrayVec::new();
//         p.try_extend_from_slice(pattern).unwrap();
//         SysexMatcher {
//             pattern: p,
//             state: Init,
//             position: 0,
//             cap_len
//         }
//     }
//
//     pub fn is_complete(&self) -> bool {
//         self.position == self.pattern.len()
//     }
//
//     /// Ignore series of bytes
//     pub fn ignore(mut self, position: usize) -> Self {
//         self.pattern[position] = 0xFF;
//         self
//     }
//
//     pub fn collect_and_reset(&mut self) {
//         // TODO collect
//         self.position = 0;
//         self.state = MatcherState::Init;
//     }
//
//     /// Returns the number of bytes added
//     pub fn match_from(&mut self, incoming: &[u8]) -> MatcherState {
//         if self.pattern.len() - self.position < incoming.len() {
//             self.state = return MatcherState::Negative;
//         }
//         if self.state != MatcherState::Negative {
//             for byte in incoming {
//                 match self.mask[self.position] {
//                     ByteMask::ExactMatch => {
//                         if self.pattern[self.position] != byte {
//                             self.state = MatcherState::Negative;
//                             break;
//                         } else {
//                             self.state = MatcherState::PartialMatch;
//                         }
//                     }
//                     ByteMask::Ignore => {}
//                     ByteMask::Capture => {
//                         self.pattern[self.position] = byte
//                     }
//                 }
//                 self.position += 1;
//             }
//             if self.state == PartialMatch && self.is_complete() {
//                 self.state = FullMatch
//             }
//         }
//         *self.state
//     }
// }
