use crate::midi::{Packet, U4};

pub type Instant = u64;
pub type Duration = u64;

#[derive(Copy, Clone, Debug, Enum)]
pub enum ButtonId {
    MAIN,
}

#[derive(Copy, Clone, Debug)]
pub enum ButtonEvent {
    Down(Instant),
    Up(Instant),
    Hold(Duration),
    Release(Duration),
}

#[derive(Copy, Clone, Debug)]
pub enum RotaryEvent {
    /// Single encoder "tick", clockwise
    TickClockwise(Instant),

    /// Single encoder "tick", counter-clockwise
    TickCounterClockwise(Instant),

    /// Value derived from encoder tick rate
    Turn(i32)
}

#[derive(Copy, Clone, Debug, Enum)]
pub enum RotaryId {
    MAIN,
}

#[derive(Copy, Clone, Debug)]
pub enum CtlEvent {
    Button(ButtonId, ButtonEvent),
    Rotary(RotaryId, RotaryEvent),
}

#[derive(Copy, Clone, Debug)]
pub enum Endpoint {
    USB,
    Serial(u8),
}

#[derive(Copy, Clone, Debug)]
pub enum MidiLane {
    Src(Endpoint),
    Dst(Endpoint),
    Route(u8),
}

#[derive(Copy, Clone, Debug)]
pub enum Config {
    MidiEcho(bool),
}

#[derive(Copy, Clone, Debug)]
pub enum Param {
    FilterCutoff(i32),
}

#[derive(Copy, Clone, Debug)]
pub enum AppEvent {
    ConfigChange(Config),
    ParamChange(Param),
}