// #![deny(unsafe_code)]
// #![deny(warnings)]
#![no_main]
#![no_std]
#![feature(alloc_error_handler)]

#![feature(const_mut_refs, slice_as_chunks)]

extern crate alloc;
extern crate cortex_m;

use alloc_cortex_m::CortexMHeap;

mod clock;
// mod global;
mod input;
mod midi;
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

use cortex_m::asm::delay;

use cortex_m_rt::entry;
use rtt_target::{rprintln, rtt_init_print};


use crate::input::Scan;
use midi::usb;
use crate::midi::{Transmit, notes, Cull, MidiError};

const SCAN_PERIOD: u32 = 200_000;
const ERROR_PERIOD: u32 = 200_000_000;
const BLINK_PERIOD: u32 = 20_000_000;

use crate::midi::serial::{SerialMidiIn, SerialMidiOut};
use crate::midi::usb::MidiClass;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::result::Result;
use cortex_m::peripheral::DWT;
use stm32f1xx_hal::serial;
use crate::midi::packet::{MidiPacket, CableNumber};
use crate::midi::Receive;
use crate::midi::message::{Channel, Velocity};
use crate::midi::message::MidiMessage::NoteOff;
use core::sync::atomic::{AtomicUsize, Ordering};

use core::alloc::Layout;
use cortex_m::asm;

use crate::state::ParamChange;
use crate::state::AppChange::{Patch, Config};

use panic_rtt_target as _;

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

// define what happens in an Out Of Memory (OOM) condition
#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    asm::bkpt();
    loop {}
}

const HEAP_SIZE: usize = 2048; // in bytes

#[app(device = stm32f1xx_hal::pac, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        inputs: Vec<Box<(dyn Scan + Sync + Send)>>,
        state: state::AppState,
        display: output::Display,
        usb_midi: midi::usb::UsbMidi,
        din_midi_in: Box<dyn Receive + Send>,
        din_midi_out: Box<dyn Transmit + Send>,
    }

    #[init(schedule = [input_scan, blink])]
    fn init(ctx: init::Context) -> init::LateResources {
        // for some RTIC reason statics need to go first
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        rtt_init_print!();

        unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }

        rprintln!("Allocator OK");

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

        rprintln!("Clocks OK");

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

        rprintln!("Blinker OK");

        // Setup Encoders
        let mut inputs = Vec::with_capacity(5);
        let encoder = input::encoder(
            input::Source::Encoder1,
            gpioa.pa6.into_pull_up_input(&mut gpioa.crl),
            gpioa.pa7.into_pull_up_input(&mut gpioa.crl),
        );
        inputs.push(encoder);

        let _enc_push = gpioa.pa5.into_pull_down_input(&mut gpioa.crl);
        ctx.schedule
            .input_scan(ctx.start + SCAN_PERIOD.cycles())
            .unwrap();

        rprintln!("Controls OK");

        // Setup Display
        let scl = gpiob.pb8.into_alternate_open_drain(&mut gpiob.crh);
        let sda = gpiob.pb9.into_alternate_open_drain(&mut gpiob.crh);

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

        // output::draw_logo(&mut oled);

        rprintln!("Screen OK");

        // Configure serial
        let tx_pin = gpioa.pa2.into_alternate_push_pull(&mut gpioa.crl);
        let rx_pin = gpioa.pa3;

        // Configure Midi
        let (tx, rx) = serial::Serial::usart2(
            peripherals.USART2,
            (tx_pin, rx_pin),
            &mut afio.mapr,
            serial::Config::default()
                .baudrate(31250.bps())
                .parity_none(),
            clocks,
            &mut rcc.apb1,
        )
            .split();
        let din_midi_out = Box::new(SerialMidiOut::new(tx));
        let din_midi_in = Box::new(SerialMidiIn::new(rx, CableNumber::MIN));

        rprintln!("Serial port OK");

        // force USB reset for dev mode (BluePill)
        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low().unwrap();
        delay(clocks.sysclk().0 / 100);

        let usb = Peripheral {
            usb: peripherals.USB,
            pin_dm: gpioa.pa11,
            pin_dp: usb_dp.into_floating_input(&mut gpioa.crh),
        };

        *USB_BUS = Some(UsbBus::new(usb));
        let midi_class = MidiClass::new(USB_BUS.as_ref().unwrap());
        let usb_dev = usb::configure_usb(USB_BUS.as_ref().unwrap());
        rprintln!("USB OK");

        rprintln!("-> Initialized");

        init::LateResources {
            inputs,
            state: state::AppState::default(),
            display: output::Display {
                onboard_led,
                oled,
                strbuf: String::with_capacity(32),
            },
            usb_midi: midi::usb::UsbMidi::new(
                usb_dev,
                midi_class,
            ),
            din_midi_in,
            din_midi_out,
        }
    }

    /// RTIC default SLEEP_ON_EXIT fucks with RTT logging, etc.
    /// Override with this NOOP idle handler
    #[idle()]
    fn idle(mut cx: idle::Context) -> ! {
        loop {}
    }

    /// USB transmit interrupt
    #[task(binds = USB_HP_CAN_TX, resources = [usb_midi], priority = 3)]
    fn usb_hp_can_tx(ctx: usb_hp_can_tx::Context) {
        if ctx.resources.usb_midi.poll() {
            rprintln!("Done sending USB")
            // TODO send more packets if any
        }
    }

    /// USB receive interrupt
    #[task(binds = USB_LP_CAN_RX0, spawn = [send_usb_midi], resources = [usb_midi], priority = 3)]
    fn usb_lp_can_rx0(ctx: usb_lp_can_rx0::Context) {
        if ctx.resources.usb_midi.poll() {
            while let Some(packet) = ctx.resources.usb_midi.receive().unwrap() {
                // rprintln!("echoing packet {:?}", packet);
                ctx.spawn.send_usb_midi(packet);
            }
        }
    }

    /// Serial receive interrupt
    #[task(binds = USART1, resources = [din_midi_in], priority = 3)]
    fn serial_rx0(ctx: serial_rx0::Context) {
        if let Some(_packet) = ctx.resources.din_midi_in.receive().unwrap() {
        }
    }

    #[task(resources = [inputs], spawn = [ctl_update], schedule = [input_scan])]
    fn input_scan(ctx: input_scan::Context) {
        let long_now = clock::long_now(DWT::get_cycle_count());
        for i in ctx.resources.inputs {
            if let Some(event) = i.scan(long_now) {
                let _change = ctx.spawn.ctl_update(event);
            }
        }

        ctx.schedule
            .input_scan(ctx.scheduled + SCAN_PERIOD.cycles())
            .unwrap();
    }

    #[task(resources = [state, display], schedule = [blink])]
    fn blink(ctx: blink::Context) {
        ctx.resources.state.ui.led_on = !ctx.resources.state.ui.led_on;
        if ctx.resources.state.ui.led_on {
            ctx.resources.display.onboard_led.set_high().unwrap();
        } else {
            ctx.resources.display.onboard_led.set_low().unwrap();
        }
        ctx.schedule
            .blink(ctx.scheduled + BLINK_PERIOD.cycles())
            .unwrap();
    }

    #[task(spawn = [redraw], resources = [state], capacity = 5)]
    fn ctl_update(ctx: ctl_update::Context, event: input::Event) {
        if let Some(change) = ctx.resources.state.ctl_update(event) {
            ctx.spawn.redraw(change);
        }
    }

    #[task(resources = [usb_midi], priority = 3)]
    fn send_usb_midi(ctx: send_usb_midi::Context, packet: MidiPacket) {
        ctx.resources.usb_midi.transmit(packet);
    }

    #[task(resources = [display])]
    fn redraw(ctx: redraw::Context, change: state::AppChange) {
        match change {
            Patch(change) => output::redraw_patch(ctx.resources.display, change),
            Config(change) => output::redraw_config(ctx.resources.display, change),
            _ => {}
        }
    }

    extern "C" {
        // Reuse some DMA interrupts for software task scheduling.
        fn DMA1_CHANNEL1();
        fn DMA1_CHANNEL2();
    }
};
