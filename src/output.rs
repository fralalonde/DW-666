// use alloc::string::String;
// use core::fmt::Write;
use heapless::{consts::*, String};
use ufmt::uwrite;
use embedded_graphics::fonts::{Font12x16};
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
use embedded_graphics::primitives::Rectangle;

pub struct Display {
    // pub strbuf: String,
    pub onboard_led: PC13<Output<PushPull>>,
    pub oled: GraphicsMode<
        I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>,
    >,
}

const PATCH_1: Point = Point::zero();
const PATCH_2: Point = Point::new(128, 16);

const CONFIG_1: Point = Point::new(0, 32);
const CONFIG_2: Point = Point::new(128, 48);

fn redraw(disp: &mut Display, text: String<U32>, top_right: Point, btm_left: Point) {
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

    Text::new(&text, top_right)
        .into_styled(text_style)
        .draw(&mut disp.oled)
        .unwrap();

    disp.oled.flush().unwrap();
}

pub fn redraw_patch(disp: &mut Display, change: super::state::ParamChange) {
    if let super::state::ParamChange::FilterCutoff(cutoff) = change {
        let mut text: String<U32> = String::new();
        uwrite!(text, "cutoff {}", cutoff).unwrap();
        redraw(disp, text, PATCH_1, PATCH_2)
    }
}

pub fn redraw_config(disp: &mut Display, change: super::state::ConfigChange) {
    if let super::state::ConfigChange::MidiEcho(echo) = change {
        let mut text: String<U32> = String::new();
        uwrite!(text, "echo {}", echo).unwrap();
        redraw(disp, text, CONFIG_1, CONFIG_2)
    }
}

//use embedded_graphics::image::{Image, ImageRaw};
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
