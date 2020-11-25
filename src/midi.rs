use stm32f1xx_hal::{
    gpio::gpioc::PC13,
    gpio::{Output, PushPull},
    pac::TIM1,
    prelude::*,
    timer::{CountDownTimer, Event, Timer},
    usb::{UsbBus, UsbBusType},
};
use usb_device::{
    bus,
    prelude::{UsbDevice, UsbDeviceState},
};
use usbd_midi::{
    data::usb_midi::usb_midi_event_packet::UsbMidiEventPacket,
    midi_device::MidiClass,
};


pub trait Midi {
    fn send(&mut self, message: UsbMidiEventPacket);
}

pub struct UsbMidi {
    pub usb_dev: UsbDevice<'static, UsbBusType>,
    pub midi_class: MidiClass<'static, UsbBusType>,
}

impl UsbMidi {
    pub fn poll(&mut self) {
        if !self.usb_dev.poll(&mut [&mut self.midi_class]) {
            return;
        }
    }
}

impl Midi for UsbMidi {
    fn send(&mut self, message: UsbMidiEventPacket) {
        // Lock this so USB interrupts don't take over
        // TODO it doesn't need to be locked
        if self.usb_dev.state() == UsbDeviceState::Configured {
            self.midi_class.send_message(message);
        }
    }
}

/*
/// Converts a button press into a usb midi packet
fn message_to_midi(
    cable: CableNumber,
    channel: Channel,
    message: Message,
) -> UsbMidiEventPacket {
    const VELOCITY: U7 = U7::MAX;
    let (button, direction) = message;
    let note = button.into();
    match direction {
        State::On => {
            let midi = MidiMessage::NoteOn(channel, note, VELOCITY);
            UsbMidiEventPacket::from_midi(cable, midi)
        }
        State::Off => {
            let midi = MidiMessage::NoteOff(channel, note, VELOCITY);
            UsbMidiEventPacket::from_midi(cable, midi)
        }
    }
}

/// Takes a old state and a new state
/// and calculates the midi events emitted transitioning between the two
/// states. Note: if a -> b -> c and called with a,c some state transitions may
/// be missed
pub fn midi_events<'a>(
    old_application: &'a ApplicationState,
    new_application: &'a ApplicationState,
) -> impl Iterator<Item = UsbMidiEventPacket> + 'a {
    let compare = move | (button,value):(&Button,&State) | -> Option<UsbMidiEventPacket> {
        let find = old_application.buttons.get(&button);
        let midi: UsbMidiEventPacket = message_to_midi(
            new_application.cable,
            new_application.channel,
            (*button,*value));
        match find {
            Some(old_value) if *old_value != *value => Some (midi),
            _ => None
        }
    };
    let events = new_application.buttons.iter().filter_map(compare);
    events
}
*/