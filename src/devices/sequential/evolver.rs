#![allow(dead_code)]

use crate::sysex::{SysexMatcher, Token, Tag};
use Token::{Seq, Cap};
use Tag::*;
use alloc::vec;

const SEQUENTIAL: u8 = 0x01;
const EVOLVER: u8 = 0x20;
const PROGRAM_PARAM: &'static [u8] = &[SEQUENTIAL, EVOLVER, 0x01, 0x01];

pub fn program_parameter_matcher() -> SysexMatcher {
    SysexMatcher::new(vec![Seq(PROGRAM_PARAM), Cap(ParamId), Cap(LsbValueU4), Cap(MsbValueU4)])
}
