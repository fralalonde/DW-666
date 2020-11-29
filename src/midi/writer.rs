use crate::midi::serial::MidiOut;
use crate::midi::message::ChannelMessage::*;
use crate::midi::message::ChannelMessage;

fn write_channel_message<T>(message: ChannelMessage, out: &mut MidiOut<T>) {
    match message {
        NoteOn(channel, note, velocity) => {
            out.write_channel_message(0x90, channel.into(), &[note.into(), velocity.into()])?;
        }
        NoteOff(channel, note, velocity) => {
            out.write_channel_message(0x80, channel.into(), &[note.into(), velocity.into()])?;
        }
        KeyPressure(channel, note, value) => {
            out.write_channel_message(0xA0, channel.into(), &[note.into(), value.into()])?;
        }
        ControlChange(channel, control, value) => {
            out.write_channel_message(0xB0, channel.into(), &[control.into(), value.into()])?;
        }
        ProgramChange(channel, program) => {
            out.write_channel_message(0xC0, channel.into(), &[program.into()])?;
        }
        ChannelPressure(channel, value) => {
            out.write_channel_message(0xD0, channel.into(), &[value.into()])?;
        }
        PitchBendChange(channel, value) => {
            let (value_lsb, value_msb) = value.into();
            out.write_channel_message(0xE0, channel.into(), &[value_lsb, value_msb])?;
        }
        QuarterFrame(value) => {
            out.tx.write(0xF1)?;
            out.tx.write(value.into())?;
        }
        SongPositionPointer(value) => {
            let (value_lsb, value_msb) = value.into();
            out.tx.write(0xF2);
            out.tx.write(value_lsb);
            out.tx.write(value_msb);
        }
        SongSelect(value) => {
            out.tx.write(0xF3)?;
            out.tx.write(value.into());
        }
        TuneRequest => {
            out.tx.write(0xF6)
        }
        TimingClock => {
            out.tx.write(0xF8)
        }
        Start => {
            out.tx.write(0xFA)
        }
        Continue => {
            out.tx.write(0xFB)
        }
        Stop => {
            out.tx.write(0xFC)
        }
        ActiveSensing => {
            out.tx.write(0xFE)
        }
        Reset => {
            out.tx.write(0xFF)        }
    }
}