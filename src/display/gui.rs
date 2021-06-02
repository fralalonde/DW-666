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
use ili9486::ILI9486;
use ili9486::gpio::GPIO8ParallelInterface;
use ili9486::io::stm32f4xx::gpioa::*;
use ili9486::io::stm32f4xx::gpiob::*;
use ili9486::io::IoPin;

use stm32f4xx_hal::{
    delay::Delay,
    gpio::{PullDown, PushPull},
};

use embedded_graphics::{
    fonts::{Font6x8},
    pixelcolor::{Rgb565, Rgb888},
    prelude::*,
    style::{PrimitiveStyle, TextStyle},
};

use ili9486::color::PixelFormat;
use ili9486::io::stm32f4xx::gpioa::GPIOA;
use ili9486::io::stm32f4xx::gpiob::GPIOB;
use ili9486::{Command, Commands};

use display_interface::v2::{ReadInterface, WriteInterface};

use embedded_graphics::primitives::{Circle};
use tinytga::Tga;
use crate::display::rotate::{Rotating, Rotation};
use lvgl::{UI, State, Color, Widget, Part, Align};
use lvgl::style::Style;
use lvgl::widgets::{Btn, Label};
use cstr_core::CString;
use crate::display::GuiError;

pub struct Display<T>
    where T: ReadInterface<u8> + WriteInterface<u8>
{
    // lcd_driver: Rotating<ILI9486<T, u8>>,
    // lcd_driver: ILI9486<T, u8>,
    ui: UI<ILI9486<T, u8>, Rgb565>
}

const SCREEN_W: u16 = 320;
const SCREEN_H: u16 = 480;
const SCREEN_BG: Rgb565 = RgbColor::BLACK;

// const MARGIN_W


const PATCH_1: Point = Point::zero();
const CONFIG_2: Point = Point::new(128, 48);

impl<T> Display<T>
    where T: ReadInterface<u8> + WriteInterface<u8>
{
    pub fn new(mut lcd_driver: ILI9486<T, u8>) -> Result<Self, GuiError> {
        let mut buffer: [u8; 0] = [0; 0];

        lcd_driver.write_command(Command::Nop, &buffer).unwrap();
        lcd_driver.write_command(Command::SleepOut, &buffer).unwrap();
        lcd_driver.write_command(Command::DisplayInversionOff, &mut buffer)?;
        lcd_driver.write_command(Command::MemoryAccessControl, &mut [0b10001000])?;

        lcd_driver.clear_screen()?;

        // Fill interface
        let mut display_info: [u8; 4] = [0; 4];
        lcd_driver.write_command(Command::ReadDisplayId, &mut [])?;
        lcd_driver.writer().read(&mut display_info)?;

        lcd_driver.write_command(Command::NormalDisplayMode, &buffer)?;
        lcd_driver.write_command(Command::DisplayOn, &buffer)?;
        lcd_driver.write_command(Command::IdleModeOff, &buffer)?;

        let mut ui = UI::init()?;

        // Implement and register your display:
        ui.disp_drv_register(lcd_driver)?;

        // Create screen and widgets
        let mut screen = ui.scr_act()?;

        let mut screen_style = Style::default();
        screen_style.set_bg_color(State::DEFAULT, Color::from_rgb((0, 0, 0)));
        screen.add_style(Part::Main, screen_style)?;

        // Create the button
        let mut button = Btn::new(&mut screen)?;
        button.set_align(&mut screen, Align::InLeftMid, 30, 0)?;
        button.set_size(180, 80)?;
        let mut btn_lbl = Label::new(&mut button)?;
        btn_lbl.set_text(CString::new("Click me!").unwrap().as_c_str())?;


        Ok(Display {
            ui
        })
    }

    pub fn print(&mut self, text: String) -> Result<(), DisplayError> {
        self.redraw(text, PATCH_1, CONFIG_2)
    }

    fn redraw(&mut self, text: String, top_left: Point, bottom_right: Point) -> Result<(), DisplayError> {
        // Rectangle::new(Point::new(16, 16), Point::new(240, 240))
        //     .into_styled(
        //         PrimitiveStyleBuilder::new()
        //             .stroke_width(32)
        //             .stroke_color(Rgb888::RED)
        //             .fill_color(Rgb888::CYAN)
        //             .build(),
        //     )
        //     .draw(&mut self.lcd_driver)
        //     .unwrap();

        // let c = Circle::new(Point::new(300, 240), 8)
        //     .into_styled(PrimitiveStyle::with_fill(Rgb888::RED));
        //
        // let t = Text::new("Hello Rust (and ILI9486 display)!", Point::new(48, 400))
        //     .into_styled(TextStyle::new(Font6x8, Rgb888::GREEN));
        //
        // c.draw(&mut self.lcd_driver).unwrap();
        // t.draw(&mut self.lcd_driver).unwrap();
        //
        // let tga = Tga::from_slice(include_bytes!("../../test/rust-rle-bw-topleft.tga")).unwrap();
        //
        // let image: Image<Tga, Rgb888> = Image::new(
        //     &tga,
        //     Point::new(
        //         (320 / 2 - (tga.width() / 2)) as i32,
        //         ((480 / 2 - (tga.height() / 2)) + 64) as i32,
        //     ),
        // );
        // image.draw(&mut self.lcd_driver).unwrap();

        Ok(())
    }

}

