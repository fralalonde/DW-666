use embedded_graphics::fonts::{Font12x16};
use embedded_graphics::prelude::Point;
use embedded_graphics::{
    fonts::Text, pixelcolor::BinaryColor, prelude::*, style::TextStyleBuilder,
};
// use ssd1306::prelude::{GraphicsMode, I2CInterface};
use embedded_graphics::style::PrimitiveStyleBuilder;
use embedded_graphics::primitives::Rectangle;

use stm32f4xx_hal::gpio::gpiob::{PB8, PB9};
use stm32f4xx_hal::gpio::{AlternateOD, AF4};
use stm32f4xx_hal::i2c::{I2c};

use embedded_graphics::image::{Image, ImageRaw};
use stm32f4xx_hal::stm32::I2C1;
use alloc::string::String;
use display_interface::DisplayError;

use stm32f4xx_hal::{
    delay::Delay,
    gpio::{PullDown, PushPull},
};

use embedded_graphics::{
    fonts::{Font6x8},
    pixelcolor::{Rgb565},
    prelude::*,
    style::{PrimitiveStyle, TextStyle},
};

use embedded_graphics::primitives::{Circle};
use tinytga::Tga;

use lvgl::{UI, State, Color, Widget, Part, Align};
use lvgl::style::Style;
use lvgl::widgets::{Btn, Label};
use cstr_core::CString;
use crate::display::GuiError;

pub struct Display<T, C>
    where T: DrawTarget<C>,
          C: RgbColor + From<Color>,
{
    ui: UI<T, C>,
}

const PATCH_1: Point = Point::zero();
const CONFIG_2: Point = Point::new(128, 48);

impl<T, C> Display<T, C>
    where T: DrawTarget<C>,
          C: RgbColor + From<Color>,
{
    pub fn new(mut lcd_driver: T) -> Result<Self, GuiError> {
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
        ui.task_handler();
        // window.update(ui.get_display_ref().unwrap());
        Ok(Display {
            ui
        })
    }

    pub fn print(&mut self, text: String) -> Result<(), DisplayError> {
        self.redraw(text, PATCH_1, CONFIG_2)
    }

    fn redraw(&mut self, text: String, top_left: Point, bottom_right: Point) -> Result<(), DisplayError> {
        self.ui.task_handler();
        // Rectangle::new(Point::new(16, 16), Point::new(240, 240))
        //     .into_styled(
        //         PrimitiveStyleBuilder::new()
        //             .stroke_width(32)
        //             .stroke_color(C::RED)
        //             .fill_color(C::CYAN)
        //             .build(),
        //     )
        //     .draw(&mut self.lcd_driver)
        //     .unwrap();

        // let c = Circle::new(Point::new(300, 240), 8)
        //     .into_styled(PrimitiveStyle::with_fill(C::RED));
        //
        // let t = Text::new("Hello Rust (and ILI9486 display)!", Point::new(48, 400))
        //     .into_styled(TextStyle::new(Font6x8, C::GREEN));
        //
        // c.draw(&mut self.lcd_driver).unwrap();
        // t.draw(&mut self.lcd_driver).unwrap();
        //
        // let tga = Tga::from_slice(include_bytes!("../../test/rust-rle-bw-topleft.tga")).unwrap();
        //
        // let image: Image<Tga, C> = Image::new(
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

