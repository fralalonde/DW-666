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
    pub onboard_led: PC13<Output<PushPull>>,
    pub oled: GraphicsMode<
        I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>,
    >,
}

const PATCH_1: Point = Point::zero();
const PATCH_2: Point = Point::new(128, 16);

const CONFIG_1: Point = Point::new(0, 32);
const CONFIG_2: Point = Point::new(128, 48);

impl Display {
    pub fn dispatch(&mut self, event: event::AppEvent) {
        match event {
            ParamChange(Param::FilterCutoff(cutoff)) => {
                let mut text: String<U32> = String::new();
                uwrite!(text, "cutoff {}", cutoff).unwrap();
                self.redraw(text, PATCH_1, PATCH_2)
            }
            ConfigChange(Config::MidiEcho(echo)) => {
                let mut text: String<U32> = String::new();
                uwrite!(text, "echo {}", echo).unwrap();
                self.redraw(text, CONFIG_1, CONFIG_2)
            }
        }
    }

    fn redraw(&mut self, text: String<U32>, top_right: Point, btm_left: Point) {
        let blank_style = PrimitiveStyleBuilder::new()
            .stroke_color(BinaryColor::Off)
            .fill_color(BinaryColor::Off)
            .build();

        Rectangle::new(top_right, btm_left)
            .into_styled(blank_style)
            .draw(&mut self.oled).unwrap();

        let text_style = TextStyleBuilder::new(Font12x16)
            .text_color(BinaryColor::On)
            .build();

        Text::new(&text, top_right)
            .into_styled(text_style)
            .draw(&mut self.oled)
            .unwrap();

        self.oled.flush().unwrap();
    }
}

use embedded_graphics::image::{Image, ImageRaw};
use crate::event;
use crate::event::{Param, Config};
use event::AppEvent::{ParamChange, ConfigChange};

pub fn draw_logo(
    oled: &mut GraphicsMode<
        I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>,
    >,
) {
    let raw: ImageRaw<BinaryColor> = ImageRaw::new(include_bytes!("../rust.raw"), 64, 64);
    let im = Image::new(&raw, Point::new(32, 0));
    im.draw(oled).unwrap();
    oled.flush().unwrap();
}
