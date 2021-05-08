// use crate::clock::{Instant, Duration};
//
// #[derive(Copy, Clone, Debug)]
// pub enum ButtonId {
//     MAIN,
// }
//
// #[derive(Copy, Clone, Debug)]
// pub enum ButtonEvent {
//     Down(Instant),
//     Up(Instant),
//     Hold(Duration),
//     Release(Duration),
// }
//
// #[derive(Copy, Clone, Debug)]
// pub enum RotaryEvent {
//     /// Single encoder "tick", clockwise
//     TickClockwise(Instant),
//
//     /// Single encoder "tick", counter-clockwise
//     TickCounterClockwise(Instant),
//
//     /// Value derived from encoder tick rate
//     Turn(i32)
// }
//
// #[derive(Copy, Clone, Debug)]
// pub enum RotaryId {
//     MAIN,
// }
//
// #[derive(Copy, Clone, Debug)]
// pub enum CtlEvent {
//     Button(ButtonId, ButtonEvent),
//     Rotary(RotaryId, RotaryEvent),
// }
//
//
// #[derive(Copy, Clone, Debug)]
// pub enum Config {
//     MidiEcho(bool),
// }
//
// #[derive(Copy, Clone, Debug)]
// pub enum Param {
//     FilterCutoff(i32),
// }
//
// #[derive(Copy, Clone, Debug)]
// pub enum AppEvent {
//     ConfigChange(Config),
//     ParamChange(Param),
// }