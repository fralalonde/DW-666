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

use embedded_graphics::image::{Image, ImageRaw};
use stm32f4xx_hal::stm32::I2C1;
use alloc::string::String;
use display_interface::DisplayError;

pub struct Display {
    pub oled: GraphicsMode<
        I2CInterface<I2c<I2C1, (PB8<AlternateOD<AF4>>, PB9<AlternateOD<AF4>>)>>,
    >,
}

const PATCH_1: Point = Point::zero();
const CONFIG_2: Point = Point::new(128, 48);

impl Display {
    pub fn print(&mut self, text: String) -> Result<(), DisplayError> {
        self.redraw(text, PATCH_1, CONFIG_2)
    }

    fn redraw(&mut self, text: String, top_left: Point, bottom_right: Point) -> Result<(), DisplayError> {
        self.oled.clear();
        self.oled.flush()?;

        let blank_style = PrimitiveStyleBuilder::new()
            .stroke_color(BinaryColor::Off)
            .fill_color(BinaryColor::Off)
            .build();

        Rectangle::new(top_left, bottom_right)
            .into_styled(blank_style)
            .draw(&mut self.oled)?;

        let text_style = TextStyleBuilder::new(Font12x16)
            .text_color(BinaryColor::On)
            .build();

        Text::new(&text, top_left)
            .into_styled(text_style)
            .draw(&mut self.oled)
            .unwrap();

        self.oled.flush()?;
        Ok(())
    }
}

pub fn draw_logo(
    oled: &mut GraphicsMode<
        I2CInterface<I2c<I2C1, (PB8<AlternateOD<AF4>>, PB9<AlternateOD<AF4>>)>>,
    >,
) -> Result<(), DisplayError> {
    let raw: ImageRaw<BinaryColor> = ImageRaw::new(include_bytes!("../rust.raw"), 64, 64);
    let im = Image::new(&raw, Point::new(32, 0));
    im.draw(oled)?;
    oled.flush()?;
    Ok(())
}
