use alloc::string::String;
use core::fmt::Write;
use embedded_graphics::fonts::{Font12x16};
use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::prelude::Point;
use embedded_graphics::{
    fonts::Text, pixelcolor::BinaryColor, prelude::*, style::TextStyleBuilder,
};
use ssd1306::prelude::{GraphicsMode, I2CInterface};
use stm32f1xx_hal::gpio::gpiob::{PB8, PB9};
use stm32f1xx_hal::gpio::gpioc::PC13;
use stm32f1xx_hal::gpio::{Alternate, OpenDrain, Output, PushPull};
use stm32f1xx_hal::i2c::{BlockingI2c};
use stm32f1xx_hal::pac::I2C1;
use embedded_graphics::style::PrimitiveStyleBuilder;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::primitives::Rectangle;
use crate::midi::MidiError;
use crate::midi::usb::PACKET_UNALIGNED;
use core::sync::atomic::Ordering::Relaxed;

pub struct Display {
    pub strbuf: String,
    pub onboard_led: PC13<Output<PushPull>>,
    pub oled: GraphicsMode<
        I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>,
    >,
}

const PATCH_1: Point = Point::zero();
const PATCH_2: Point = Point::new(128, 16);

// const MIDI_1: Point = Point::new(0, 16);
// const MIDI_2: Point = Point::new(128, 32);

const CONFIG_1: Point = Point::new(0, 32);
const CONFIG_2: Point = Point::new(128, 48);

const ERROR_1: Point = Point::new(0, 48);
const ERROR_2: Point = Point::new(128, 64);

fn redraw(disp: &mut Display, top_right: Point, btm_left: Point) {
    let blank_style = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::Off)
        .fill_color(BinaryColor::Off)
        .build();

    Rectangle::new(top_right, btm_left)
        .into_styled(blank_style)
        .draw(&mut disp.oled).unwrap();

    let text_style = TextStyleBuilder::new(Font12x16)
        .text_color(BinaryColor::On)
        .build();

    Text::new(&disp.strbuf, top_right)
        .into_styled(text_style)
        .draw(&mut disp.oled)
        .unwrap();

    disp.oled.flush().unwrap();
}

// pub fn redraw_midi(disp: &mut Display, change: super::state::) {
//     disp.strbuf.clear();
//     write!(disp.strbuf, "midi {}", cutoff).unwrap();
//     redraw(disp, &disp.strbuf, MIDI_1, MIDI_2)
// }

pub fn redraw_patch(disp: &mut Display, change: super::state::ParamChange) {
    if let super::state::ParamChange::FilterCutoff(cutoff) = change {
        disp.strbuf.clear();
        write!(disp.strbuf, "cutoff {}", cutoff).unwrap();
        redraw(disp, PATCH_1, PATCH_2)
    }
}

pub fn redraw_config(disp: &mut Display, change: super::state::ConfigChange) {
    if let super::state::ConfigChange::MidiEcho(echo) = change {
        disp.strbuf.clear();
        write!(disp.strbuf, "echo {}", echo).unwrap();
        redraw(disp, CONFIG_1, CONFIG_2)
    }
}

pub fn redraw_error(disp: &mut Display, error: MidiError) {
    disp.strbuf.clear();
    match error {
        MidiError::PayloadOverflow => write!(disp.strbuf, "PayloadOverflow"),
        MidiError::SysexInterrupted => write!(disp.strbuf, "SysexInterrupted"),
        MidiError::NotAMidiStatus => write!(disp.strbuf, "NotAMidiStatus"),
        MidiError::NotAChanelCommand => write!(disp.strbuf, "NotAChanelCommand"),
        MidiError::NotASystemCommand => write!(disp.strbuf, "NotASystemCommand"),
        MidiError::UnhandledDecode => write!(disp.strbuf, "UnhandledDecode"),
        MidiError::SysexOutOfBounds => write!(disp.strbuf, "SysexOutOfBounds"),
        MidiError::InvalidU4 => write!(disp.strbuf, "InvalidU4"),
        MidiError::InvalidU7 => write!(disp.strbuf, "InvalidU7"),
        MidiError::InvalidU14 => write!(disp.strbuf, "InvalidU14"),
        MidiError::SerialError => write!(disp.strbuf, "SerialError"),
        MidiError::UsbError => write!(disp.strbuf, "UsbError"),
        MidiError::UsbLeftover(bytes) => write!(disp.strbuf, "Usb Leftovers {}", bytes),
    }.unwrap();

    redraw(disp, ERROR_1, ERROR_2)
}

pub fn redraw_fault(disp: &mut Display) {
    disp.strbuf.clear();
    write!(disp.strbuf, "USB_XRUN {}", PACKET_UNALIGNED.fetch_max(0, Relaxed));
    redraw(disp, ERROR_1, ERROR_2)
}

// pub fn draw_logo(
//     oled: &mut GraphicsMode<
//         I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>,
//     >,
// ) {
//     let raw: ImageRaw<BinaryColor> = ImageRaw::new(include_bytes!("../rust.raw"), 64, 64);
//     let im = Image::new(&raw, Point::new(32, 0));
//     im.draw(oled).unwrap();
//     oled.flush().unwrap();
// }
