// #![deny(unsafe_code)]
// #![deny(warnings)]
#![no_main]
#![no_std]
#![feature(alloc_error_handler)]

#![feature(const_mut_refs, slice_as_chunks)]

extern crate alloc;
extern crate cortex_m;
// extern crate panic_semihosting;

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

use crate::input::Scan;
use midi::usb;
use crate::midi::{Transmit, notes, Cull};

const SCAN_PERIOD: u32 = 200_000;
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

use defmt_rtt as _; // global logger

use core::alloc::Layout;
use cortex_m::asm;

use panic_probe as _;

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

#[defmt::timestamp]
fn timestamp() -> u64 {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    // NOTE(no-CAS) `timestamps` runs with interrupts disabled
    let n = COUNT.load(Ordering::Relaxed);
    COUNT.store(n + 1, Ordering::Relaxed);
    n as u64
}

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
        state: state::ApplicationState,
        display: output::Display,
        usb_midi: midi::usb::UsbMidi,
        din_midi_in: Box<dyn Receive + Send>,
        din_midi_out: Box<dyn Transmit + Send>,
    }

    #[init(schedule = [input_scan, blink])]
    fn init(ctx: init::Context) -> init::LateResources {
        // for some RTIC reason statics need to go first
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }

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

        output::draw_logo(&mut oled);

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

        init::LateResources {
            inputs,
            state: state::ApplicationState::default(),
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

    // High priority USB interrupts
    #[task(binds = USB_HP_CAN_TX, resources = [usb_midi], priority = 3)]
    fn usb_hp_can_tx(ctx: usb_hp_can_tx::Context) {
        if ctx.resources.usb_midi.poll() {
            // TODO send more packets if any
        }
    }

    // Low priority USB interrupts
    #[task(binds = USB_LP_CAN_RX0, spawn = [send_usb_midi], resources = [usb_midi], priority = 3)]
    fn usb_lp_can_rx0(ctx: usb_lp_can_rx0::Context) {
        if ctx.resources.usb_midi.poll() {
            match ctx.resources.usb_midi.receive() {
                Ok(Some(packet)) => {
                    if let Err(err) = ctx.spawn.send_usb_midi(packet) {
                        defmt::warn!("usb midi echo failed {:?}", err)
                    }
                }
                _ => {}
            }
        }
    }

    // DIN MIDI interrupts
    #[task(binds = USART1, resources = [din_midi_in], priority = 3)]
    fn serial_rx0(ctx: serial_rx0::Context) {
        if let Err(_err) = ctx.resources.din_midi_in.receive() {}
        // TODO read & dispatch packet
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

    #[task(resources = [state, display], spawn = [update], schedule = [blink])]
    fn blink(ctx: blink::Context) {
        if ctx.resources.state.led_on {
            ctx.resources.display.onboard_led.set_high().unwrap();
            ctx.resources.state.led_on = false;
        } else {
            ctx.resources.display.onboard_led.set_low().unwrap();
            ctx.resources.state.led_on = true;
        }
        ctx.schedule
            .blink(ctx.scheduled + BLINK_PERIOD.cycles())
            .unwrap();
    }

    #[task(spawn = [redraw, send_usb_midi], resources = [state], capacity = 5)]
    fn update(ctx: update::Context, event: input::Event) {
        if let Some(change) = ctx.resources.state.update(event) {
            if let Err(err) = ctx.spawn.redraw(change) {
                defmt::warn!("redraw failed {:?}", err)
            }

            if let Err(err) = ctx.spawn.send_usb_midi(MidiPacket::from_message(
                CableNumber::MIN,
                NoteOff(Channel::cull(1), notes::Note::C1m, Velocity::MAX),
            )) {
                defmt::warn!("midi failed {:?}", err)
            }
        }
    }

    #[task(resources = [usb_midi], priority = 3)]
    fn send_usb_midi(ctx: send_usb_midi::Context, packet: MidiPacket) {
        if let Err(e) = ctx.resources.usb_midi.transmit(packet) {
            defmt::warn!("Serial send failed {:?}", e);
        }
    }

    #[task(resources = [display])]
    fn redraw(ctx: redraw::Context, change: state::StateChange) {
        output::redraw(ctx.resources.display, change);
    }

    extern "C" {
        // Reuse some DMA interrupts for software task scheduling.
        fn DMA1_CHANNEL1();
        fn DMA1_CHANNEL2();
    }
};
