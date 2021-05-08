use heapless::{String};
use ufmt::uwrite;
use embedded_graphics::fonts::{Font12x16};
use embedded_graphics::prelude::Point;
use embedded_graphics::{
    fonts::Text, pixelcolor::BinaryColor, prelude::*, style::TextStyleBuilder,
};
use ssd1306::prelude::{GraphicsMode, I2CInterface};
use embedded_graphics::style::PrimitiveStyleBuilder;
use embedded_graphics::primitives::Rectangle;

use stm32f4xx_hal::gpio::gpiob::{PB8, PB9};
use stm32f4xx_hal::gpio::{AlternateOD, AF4};
use stm32f4xx_hal::i2c::{I2c};

pub struct Display {
    pub oled: GraphicsMode<
        I2CInterface<I2c<I2C1, (PB8<AlternateOD<AF4>>, PB9<AlternateOD<AF4>>)>>,
    >,
}

const PATCH_1: Point = Point::zero();
const PATCH_2: Point = Point::new(128, 16);

const CONFIG_1: Point = Point::new(0, 32);
const CONFIG_2: Point = Point::new(128, 48);

impl Display {
    pub fn update(&mut self) {
        // match event {
        //     ParamChange(Param::FilterCutoff(cutoff)) => {
        //         let mut text: String<32> = String::new();
        //         uwrite!(text, "cutoff {}", cutoff).unwrap();
        //         self.redraw(text, PATCH_1, PATCH_2)
        //     }
        //     ConfigChange(Config::MidiEcho(echo)) => {
        //         let mut text: String<32> = String::new();
        //         uwrite!(text, "echo {}", echo).unwrap();
        //         self.redraw(text, CONFIG_1, CONFIG_2)
        //     }
        // }
    }

    fn redraw(&mut self, text: String<32>, top_right: Point, btm_left: Point) {
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
use stm32f4xx_hal::stm32::I2C1;

pub fn draw_logo(
    oled: &mut GraphicsMode<
        I2CInterface<I2c<I2C1, (PB8<AlternateOD<AF4>>, PB9<AlternateOD<AF4>>)>>,
    >,
) {
    let raw: ImageRaw<BinaryColor> = ImageRaw::new(include_bytes!("../rust.raw"), 64, 64);
    let im = Image::new(&raw, Point::new(32, 0));
    im.draw(oled).unwrap();
    oled.flush().unwrap();
}
