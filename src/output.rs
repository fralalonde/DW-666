use alloc::string::String;
use core::fmt::Write;
use embedded_graphics::fonts::{Font12x16, Font24x32};
use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::prelude::Point;
use embedded_graphics::{
    fonts::Text, pixelcolor::BinaryColor, prelude::*, style::TextStyleBuilder,
};
use embedded_hal::digital::OutputPin;
use ssd1306::prelude::{GraphicsMode, I2CInterface};
use ssd1306::{Builder, I2CDIBuilder};
use stm32f1xx_hal::gpio::gpiob::{PB8, PB9};
use stm32f1xx_hal::gpio::gpioc::PC13;
use stm32f1xx_hal::gpio::{Alternate, OpenDrain, Output, PushPull};
use stm32f1xx_hal::i2c::{BlockingI2c, DutyCycle, Mode};
use stm32f1xx_hal::pac::I2C1;
use stm32f1xx_hal::time::U32Ext;
use stm32f1xx_hal::usb::Peripheral;

pub struct Display {
    pub strbuf: String,
    pub onboard_led: PC13<Output<PushPull>>,
    pub oled: GraphicsMode<
        I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>,
    >,
}

pub fn redraw(disp: &mut Display, change: super::state::StateChange) {
    if let super::state::StateChange::Value(current_count) = change {
        let text_style = TextStyleBuilder::new(Font12x16)
            .text_color(BinaryColor::On)
            .build();

        disp.strbuf.clear();
        write!(disp.strbuf, "enc_val\n{}", current_count).unwrap();

        disp.oled.clear();

        Text::new(&disp.strbuf, Point::zero())
            .into_styled(text_style)
            .draw(&mut disp.oled)
            .unwrap();

        disp.oled.flush().unwrap();
    }
}

pub fn draw_logo(
    oled: &mut GraphicsMode<
        I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>
    >,
) {
    let raw: ImageRaw<BinaryColor> = ImageRaw::new(include_bytes!("./rust.raw"), 64, 64);
    let im = Image::new(&raw, Point::new(32, 0));
    im.draw(oled).unwrap();
    oled.flush().unwrap();
}
