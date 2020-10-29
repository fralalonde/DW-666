#![deny(unsafe_code)]
// #![deny(warnings)]
#![no_main]
#![no_std]

extern crate panic_semihosting;

// mod hid;
// use hid::HIDClass;

use embedded_hal::digital::v2::{OutputPin, InputPin};
use rtic::app;
use rtic::cyccnt::{Instant, U32Ext as _, Duration};

use stm32f1xx_hal::gpio::{gpioc::PC13, Output, PushPull, State, Input, PullDown, Alternate, OpenDrain, PullUp};
use stm32f1xx_hal::prelude::*;
use stm32f1xx_hal::i2c::{DutyCycle, Mode, BlockingI2c};

use embedded_graphics::{
    style::TextStyleBuilder,
    fonts::{Text},
    // image::{Image, ImageRaw},
    pixelcolor::BinaryColor,
    prelude::*,
};
use embedded_graphics::fonts::Font24x32;

use ssd1306::{prelude::*, Builder, I2CDIBuilder};
use stm32f1xx_hal::gpio::gpioa::{PA5, PA6, PA7};
use stm32f1xx_hal::pac::I2C1;
use stm32f1xx_hal::gpio::gpiob::{PB8, PB9};
use stm32f1xx_hal::usb::{Peripheral, UsbBus, UsbBusType};

use usb_device::bus;
use usb_device::prelude::*;
use usbd_serial::{SerialPort, USB_CLASS_CDC};

use core::fmt::Write;
use heapless::String;
use heapless::consts::*;
use cortex_m::asm::delay;

const SCAN_PERIOD: u32 = 200_000;
const PRINT_PERIOD: u32 = 2_000_000;
const BLINK_PERIOD: u32 = 20_000_000;
const CYCLES_STEPPING: u32 = 2_000_000;

#[app(device = stm32f1xx_hal::pac, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        onboard_led: PC13<Output<PushPull>>,
        enc_push: PA5<Input<PullDown>>,
        enc_dt: PA6<Input<PullUp>>,
        enc_clk: PA7<Input<PullUp>>,
        disp: GraphicsMode<I2CInterface<BlockingI2c<I2C1, (PB8<Alternate<OpenDrain>>, PB9<Alternate<OpenDrain>>)>>>,
        
        led_blink: bool,
        enc_count: u8,
        // dt, clk
        enc_last_pos: (bool, bool),
        enc_last_time: Instant,

        serial_dev: UsbDevice<'static, UsbBusType>,
        serial: SerialPort<'static, UsbBusType>,

        // mouse_dev: UsbDevice<'static, UsbBusType>,
        // hid: HIDClass<'static, UsbBusType>,

    }

    #[init(schedule = [blinker, scanner, printer])]
    fn init(ctx: init::Context) -> init::LateResources {
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        // Enable cycle counter
        let mut core = ctx.core;
        core.DWT.enable_cycle_counter();

        let device: stm32f1xx_hal::stm32::Peripherals = ctx.device;

        // Setup clocks
        let mut flash = device.FLASH.constrain();
        let mut rcc = device.RCC.constrain();
        let mut afio = device.AFIO.constrain(&mut rcc.apb2);
        let clocks = rcc
            .cfgr
            .use_hse(8.mhz())
            .sysclk(72.mhz())
            .pclk1(36.mhz())
            .freeze(&mut flash.acr);

        assert!(clocks.usbclk_valid());

        // Setup LED
        let mut gpioc = device.GPIOC.split(&mut rcc.apb2);
        let mut onboard_led = gpioc
            .pc13
            .into_push_pull_output_with_state(&mut gpioc.crh, State::Low);
        onboard_led.set_low().unwrap();
        ctx.schedule.blinker(ctx.start + BLINK_PERIOD.cycles()).unwrap();

        // Setup Encoder
        let mut gpioa = device.GPIOA.split(&mut rcc.apb2);
        let enc_push = gpioa.pa5.into_pull_down_input(&mut gpioa.crl);
        let led_blink = enc_push.is_low().unwrap();
        let enc_dt = gpioa.pa6.into_pull_up_input(&mut gpioa.crl);
        let enc_clk = gpioa.pa7.into_pull_up_input(&mut gpioa.crl);
        let enc_last_pos = (enc_dt.is_low().unwrap(), enc_clk.is_low().unwrap());
        ctx.schedule.scanner(ctx.start + SCAN_PERIOD.cycles()).unwrap();

        // Setup Display
        let mut gpiob = device.GPIOB.split(&mut rcc.apb2);
        let scl = gpiob.pb8.into_alternate_open_drain(&mut gpiob.crh);
        let sda = gpiob.pb9.into_alternate_open_drain(&mut gpiob.crh);
        let i2c = BlockingI2c::i2c1(
            device.I2C1, (scl, sda), &mut afio.mapr,
            Mode::Fast {
                frequency: 400_000.hz(),
                duty_cycle: DutyCycle::Ratio2to1,
            },
            clocks, &mut rcc.apb1, 1000, 10, 1000, 1000,);
        let interface = I2CDIBuilder::new().init(i2c);
        let mut disp: GraphicsMode<_> = Builder::new().connect(interface).into();
        disp.init().unwrap();

        ctx.schedule.printer(ctx.start + PRINT_PERIOD.cycles()).unwrap();

        // BluePill board has a pull-up resistor on the D+ line.
        // Pull the D+ pin down to send a RESET condition to the USB bus.
        // This forced reset is needed only for development, without it host
        // will not reset your device when you upload new firmware.
        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low().unwrap();
        delay(clocks.sysclk().0 / 100);

        let usb_dm = gpioa.pa11;
        let usb_dp = usb_dp.into_floating_input(&mut gpioa.crh);

        let usb = Peripheral {
            usb: device.USB,
            pin_dm: usb_dm,
            pin_dp: usb_dp,
        };

        *USB_BUS = Some(UsbBus::new(usb));

        let serial = SerialPort::new(USB_BUS.as_ref().unwrap());

        let serial_dev = UsbDeviceBuilder::new(USB_BUS.as_ref().unwrap(), UsbVidPid(0x16c0, 0x27dd))
            .manufacturer("Fake company")
            .product("Serial port")
            .serial_number("TEST")
            .device_class(USB_CLASS_CDC)
            .build();

        // let hid = HIDClass::new(USB_BUS.as_ref().unwrap());
        // let mouse_dev = UsbDeviceBuilder::new(USB_BUS.as_ref().unwrap(), UsbVidPid(0xc410, 0x0000))
        //     .manufacturer("Fake company")
        //     .product("mouse")
        //     .serial_number("TEST")
        //     .device_class(0)
        //     .build();

        /////

        init::LateResources {
            // Devices
            onboard_led,
            enc_push,
            enc_dt,
            enc_clk,
            disp,

            serial_dev,
            serial,

            // hid,
            // mouse_dev,

            // State
            led_blink,
            enc_last_pos,
            enc_last_time: ctx.start,
            enc_count: 0,
        }
    }

    #[task(binds = USB_HP_CAN_TX, resources = [serial_dev, serial])]
    fn usb_tx(mut ctx: usb_tx::Context) {
        usb_poll(&mut ctx.resources.serial_dev, &mut ctx.resources.serial);
    }

    #[task(binds = USB_LP_CAN_RX0, resources = [serial_dev, serial])]
    fn usb_rx0(mut ctx: usb_rx0::Context) {
        usb_poll(&mut ctx.resources.serial_dev, &mut ctx.resources.serial);
    }

    #[task(resources = [onboard_led, led_blink, enc_count,], schedule = [blinker])]
    fn blinker(ctx: blinker::Context) {
        // Use the safe local `static mut` of RTIC
        static mut LED_STATE: bool = false;

        if !*ctx.resources.led_blink {
            if *LED_STATE {
                ctx.resources.onboard_led.set_high().unwrap();
                *LED_STATE = false;
            } else {
                ctx.resources.onboard_led.set_low().unwrap();
                *LED_STATE = true;
            }
        }
        ctx.schedule.blinker(ctx.scheduled + BLINK_PERIOD.cycles()).unwrap();
    }

    #[task(resources = [enc_push, enc_dt, enc_clk, enc_last_pos, enc_last_time, enc_count, led_blink], schedule = [scanner])]
    fn scanner(ctx: scanner::Context) {
        let enc_state = ctx.resources.enc_push.is_low().unwrap();
        if  enc_state != *ctx.resources.led_blink {
            *ctx.resources.led_blink = enc_state;
        }

        let enc_code =
            (ctx.resources.enc_dt.is_low().unwrap(), ctx.resources.enc_clk.is_low().unwrap());
        let enc_last_pos = *ctx.resources.enc_last_pos;
        if enc_code != enc_last_pos {
            let elap: Duration = ctx.scheduled - *ctx.resources.enc_last_time;
            // exponential stepping based on rotation speed
            let steps = match elap.as_cycles() / CYCLES_STEPPING {
                // 0 => 16,
                0..=1 => 16,
                2..=4 => 4,
                    // 4..=6 => 2,
                _ => 1,
            };
            match (enc_last_pos, enc_code) {
                ((true, false), (true, true)) => {
                    *ctx.resources.enc_count -= steps;
                    *ctx.resources.enc_last_time = ctx.scheduled;
                }
                ((false, true), (true, true)) => {
                    *ctx.resources.enc_count += steps;
                    *ctx.resources.enc_last_time = ctx.scheduled;
                }
                _ => {}
            };
            *ctx.resources.enc_last_pos = enc_code;
        }


        ctx.schedule.scanner(ctx.scheduled + SCAN_PERIOD.cycles()).unwrap();
    }

    #[task(resources = [disp, enc_count], schedule = [printer])]
    fn printer(ctx: printer::Context) {
        static mut last_count: u8 = 0;
        let mut strbuf = String::<U32>::new();

        let current_count = *ctx.resources.enc_count;
        if current_count != *last_count {
            let text_style = TextStyleBuilder::new(Font24x32)
                .text_color(BinaryColor::On)
                .build();

            strbuf.clear();
            write!(strbuf, "{}", current_count).unwrap();

            let disp = ctx.resources.disp;
            disp.clear();

            // let raw: ImageRaw<BinaryColor> = ImageRaw::new(include_bytes!("./rust.raw"), 64, 64);
            // let im = Image::new(&raw, Point::new(32, 0));
            // im.draw(disp).unwrap();
            // let mut delay = Delay::new(cp.SYST, clocks);

            Text::new(&strbuf, Point::zero())
                .into_styled(text_style)
                .draw(disp)
                .unwrap();

            disp.flush().unwrap();

            *last_count = current_count;
        }

        ctx.schedule.printer(ctx.scheduled + PRINT_PERIOD.cycles()).unwrap();
    }

    extern "C" {
        fn EXTI0();
    }
};

fn usb_poll<B: bus::UsbBus>(
    usb_dev: &mut UsbDevice<'static, B>,
    serial: &mut SerialPort<'static, B>,
) {
    if !usb_dev.poll(&mut [serial]) {
        return;
    }

    let mut buf = [0u8; 8];

    match serial.read(&mut buf) {
        Ok(count) if count > 0 => {
            // Echo back in upper case
            for c in buf[0..count].iter_mut() {
                if 0x61 <= *c && *c <= 0x7a {
                    *c &= !0x20;
                }
            }

            serial.write(&buf[0..count]).ok();
        }
        _ => {}
    }
}