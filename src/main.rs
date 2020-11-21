// #![deny(unsafe_code)]
// #![deny(warnings)]
#![no_main]
#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;
extern crate cortex_m;
extern crate panic_semihosting;

mod clock;
mod global;
mod input;
mod output;
mod state;

use embedded_hal::digital::v2::OutputPin;
use rtic::app;
use rtic::cyccnt::U32Ext as _;

use stm32f1xx_hal::gpio::State;
use stm32f1xx_hal::i2c::{BlockingI2c, DutyCycle, Mode};
use stm32f1xx_hal::prelude::*;

use ssd1306::{prelude::*, Builder, I2CDIBuilder};
use stm32f1xx_hal::usb::{Peripheral, UsbBus, UsbBusType};

use usb_device::bus;
use usb_device::prelude::*;

use cortex_m::asm::delay;

use crate::input::Scan;
use usbd_midi::data::usb::constants::USB_CLASS_NONE;
use usbd_midi::midi_device::MidiClass;

const SCAN_PERIOD: u32 = 200_000;
const BLINK_PERIOD: u32 = 20_000_000;

use crate::state::StateChange;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use cortex_m::peripheral::DWT;
use embedded_graphics::image::{Image, ImageRaw};

#[app(device = stm32f1xx_hal::pac, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        inputs: Vec<Box<(dyn Scan + Sync + Send)>>,

        state: state::ApplicationState,

        output: output::Display,

        midi_dev: UsbDevice<'static, UsbBusType>,
        midi_class: MidiClass<'static, UsbBusType>,
    }

    #[init(schedule = [input_scan, blink])]
    fn init(ctx: init::Context) -> init::LateResources {
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        // Enable cycle counter
        let mut core = ctx.core;
        core.DWT.enable_cycle_counter();

        let peripherals: stm32f1xx_hal::stm32::Peripherals = ctx.device;

        // Setup clocks
        let mut flash = peripherals.FLASH.constrain();
        let mut rcc = peripherals.RCC.constrain();
        let mut afio = peripherals.AFIO.constrain(&mut rcc.apb2);
        let clocks = rcc
            .cfgr
            .use_hse(8.mhz())
            // maximum CPU overclock
            .sysclk(72.mhz())
            .pclk1(36.mhz())
            .freeze(&mut flash.acr);

        assert!(clocks.usbclk_valid());

        // Get GPIO busses
        let mut gpioa = peripherals.GPIOA.split(&mut rcc.apb2);
        let mut gpiob = peripherals.GPIOB.split(&mut rcc.apb2);
        let mut gpioc = peripherals.GPIOC.split(&mut rcc.apb2);

        // // Setup LED
        let mut onboard_led = gpioc
            .pc13
            .into_push_pull_output_with_state(&mut gpioc.crh, State::Low);
        onboard_led.set_low().unwrap();
        ctx.schedule
            .blink(ctx.start + BLINK_PERIOD.cycles())
            .unwrap();

        // // Setup Encoder
        let mut inputs = Vec::with_capacity(5);
        let mut encoder = input::encoder(
            0,
            gpioa.pa6.into_pull_up_input(&mut gpioa.crl),
            gpioa.pa7.into_pull_up_input(&mut gpioa.crl),
        );
        inputs.push(encoder);

        let _enc_push = gpioa.pa5.into_pull_down_input(&mut gpioa.crl);
        ctx.schedule
            .input_scan(ctx.start + SCAN_PERIOD.cycles())
            .unwrap();

        // Setup Display
        let scl = gpiob.pb8.into_alternate_open_drain(&mut gpiob.crh);
        let sda = gpiob.pb9.into_alternate_open_drain(&mut gpiob.crh);

        // TODO extract i2c, oled builders
        let i2c = BlockingI2c::i2c1(
            peripherals.I2C1,
            (scl, sda),
            &mut afio.mapr,
            Mode::Fast {
                frequency: 400_000.hz(),
                duty_cycle: DutyCycle::Ratio2to1,
            },
            clocks,
            &mut rcc.apb1,
            1000,
            10,
            1000,
            1000,
        );
        let oled_i2c = I2CDIBuilder::new().init(i2c);
        let mut oled: GraphicsMode<_> = Builder::new().connect(oled_i2c).into();
        oled.init().unwrap();

        output::draw_logo(&mut oled);

        // BluePill board has a pull-up resistor on the D+ line.
        // Pull the D+ pin down to send a RESET condition to the USB bus.
        // This forced reset is needed only for development, without it host
        // will not reset your device when you upload new firmware.
        // let mut delay = Delay::new(cp.SYST, clocks);
        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low().unwrap();
        delay(clocks.sysclk().0 / 100);

        let usb_dm = gpioa.pa11;
        let usb_dp = usb_dp.into_floating_input(&mut gpioa.crh);

        let usb = Peripheral {
            usb: peripherals.USB,
            pin_dm: usb_dm,
            pin_dp: usb_dp,
        };

        *USB_BUS = Some(UsbBus::new(usb));

        let midi_class = MidiClass::new(USB_BUS.as_ref().unwrap());
        let midi_dev = UsbDeviceBuilder::new(USB_BUS.as_ref().unwrap(), UsbVidPid(0x16c0, 0x27dd))
            .manufacturer("Roto")
            .product("USB MIDI Router")
            .serial_number("123")
            .device_class(USB_CLASS_NONE)
            .build();

        init::LateResources {
            inputs,

            // Devices
            output: output::Display {
                onboard_led,
                oled,
                strbuf: String::with_capacity(32),
            },

            midi_class,
            midi_dev,

            state: state::ApplicationState::default(),
        }
    }

    // Process usb events straight away from High priority interrupts
    #[task(binds = USB_HP_CAN_TX,resources = [midi_dev, midi_class], priority=3)]
    fn usb_hp_can_tx(ctx: usb_hp_can_tx::Context) {
        if !ctx.resources.midi_dev.poll(&mut [ctx.resources.midi_class]) {
            return;
        }
    }

    // Process usb events straight away from Low priority interrupts
    #[task(binds= USB_LP_CAN_RX0, resources = [midi_dev, midi_class], priority=3)]
    fn usb_lp_can_rx0(ctx: usb_lp_can_rx0::Context) {
        if !ctx.resources.midi_dev.poll(&mut [ctx.resources.midi_class]) {
            return;
        }
    }

    #[task(resources = [inputs], spawn = [update], schedule = [input_scan])]
    fn input_scan(ctx: input_scan::Context) {
        let long_now = clock::long_now(DWT::get_cycle_count());
        for i in ctx.resources.inputs {
            if let Some(event) = i.scan(long_now) {
                let _err = ctx.spawn.update(event);
            }
        }

        ctx.schedule
            .input_scan(ctx.scheduled + SCAN_PERIOD.cycles())
            .unwrap();
    }

    #[task(resources = [state, output], spawn = [update], schedule = [blink])]
    fn blink(ctx: blink::Context) {
        if ctx.resources.state.led_on {
            ctx.resources.output.onboard_led.set_high().unwrap();
            ctx.resources.state.led_on = false;
        } else {
            ctx.resources.output.onboard_led.set_low().unwrap();
            ctx.resources.state.led_on = true;
        }
        ctx.schedule
            .blink(ctx.scheduled + BLINK_PERIOD.cycles())
            .unwrap();
    }

    #[task( spawn = [redraw], resources = [state], capacity = 5)]
    fn update(ctx: update::Context, event: input::ScanEvent) {
        match event {
            input::ScanEvent::Encoder(z) => {
                ctx.resources.state.enc_count += z;
                ctx.spawn.redraw(StateChange::Value(ctx.resources.state.enc_count));
            }
            _ => {}
        }
    }

    #[task(resources = [output])]
    fn redraw(ctx: redraw::Context, change: state::StateChange) {
        output::redraw(ctx.resources.output, change);
    }

    extern "C" {
        // fn EXTI0();
        // Divert DMA1_CHANNELX interrupts for software task scheduling.
        fn DMA1_CHANNEL1();
        fn DMA1_CHANNEL2();
    }
};

/*
/// Will be called periodically.
#[task(binds = TIM1_UP,
spawn = [update],
resources = [inputs,timer],
priority = 1)]
fn read_inputs(cx: read_inputs::Context) {
    // There must be a better way to bank over
    // these below checks

    let values = read_input_pins(cx.resources.inputs);

    let _ = cx.spawn.update((Button::One, values.pin1));
    let _ = cx.spawn.update((Button::Two, values.pin2));
    let _ = cx.spawn.update((Button::Three, values.pin3));
    let _ = cx.spawn.update((Button::Four, values.pin4));
    let _ = cx.spawn.update((Button::Five, values.pin5));

    cx.resources.timer.clear_update_interrupt_flag();
}

#[task( spawn = [send_midi],
resources = [state],
priority = 1,
capacity = 5)]
fn update(cx: update::Context, message: state::Message) {
    let old = cx.resources.state.clone();
    ApplicationState::update(&mut *cx.resources.state, message);
    let mut effects = midi_events(&old, cx.resources.state);
    let effect = effects.next();

    match effect {
        Some(midi) => {
            let _ = cx.spawn.send_midi(midi);
        }
        _ => (),
    }
}

/// Sends a midi message over the usb bus
/// Note: this runs at a lower priority than the usb bus
/// and will eat messages if the bus is not configured yet
#[task(priority=2, resources = [usb_dev,midi])]
fn send_midi(cx: send_midi::Context, message: UsbMidiEventPacket) {
    let mut midi = cx.resources.midi;
    let mut usb_dev = cx.resources.usb_dev;

    // Lock this so USB interrupts don't take over
    // Ideally we may be able to better determine this, so that
    // it doesn't need to be locked
    usb_dev.lock(|usb_dev| {
        if usb_dev.state() == UsbDeviceState::Configured {
            midi.lock(|midi| {
                let _ = midi.send_message(message);
            })
        }
    });
}
 */
