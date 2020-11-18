use alloc::string::String;
use ssd1306::prelude::{GraphicsMode, I2CInterface};
use stm32f1xx_hal::i2c::BlockingI2c;
use stm32f1xx_hal::pac::I2C1;
use stm32f1xx_hal::gpio::gpiob::{PB8, PB9};
use stm32f1xx_hal::gpio::{Alternate, OpenDrain, Output, PushPull};
use stm32f1xx_hal::gpio::gpioc::PC13;

pub struct Display {
    pub strbuf: String,
    pub onboard_led: PC13<Output<PushPull>>,
    pub disp: GraphicsMode<I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>>,
}