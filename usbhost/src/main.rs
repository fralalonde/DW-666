#![no_std]
#![no_main]

#[macro_use]
extern crate log;

#[macro_use]
extern crate rtt_target;

use panic_rtt_target as _;

use trinket_m0 as hal;

use hal::clock::GenericClockController;
use hal::entry;
use hal::pac::{interrupt, CorePeripherals, Peripherals};

use cortex_m::asm::delay as cycle_delay;
use cortex_m::peripheral::{NVIC, DWT};
use atsamd_hal::time::Hertz;

use atsamd_hal::gpio::v2::PA10;
use atsamd_hal::delay::Delay;
use atsamd_hal::hal::blocking::delay::DelayMs;
use atsamd_hal::sercom::UART0;

use atsamd_usb_host::{SAMDHost, Pins, Event, Events};

use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::Relaxed;

use heapless::Vec;
use crate::midi_driver::MidiDriver;
use log::LevelFilter;

mod midi_driver;

static mut MILLIS: AtomicUsize = AtomicUsize::new(0);

fn millis() -> usize {
    // FIXME this will rollover quickly...
    unsafe { MILLIS.load(Relaxed) }
}

use log::{Metadata, Record};

use rtt_target::*;
use atsamd_hal::common::sercom::v2::{Pad, Pad3, Pad2};

/// An RTT-based logger implementation.
pub struct RTTLogger {}

impl log::Log for RTTLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            rprintln!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static MY_LOGGER: RTTLogger = RTTLogger{};

#[rtic::app(device = crate::hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        red_led: atsamd_hal::gpio::Pin<PA10, atsamd_hal::gpio::v2::Output<atsamd_hal::gpio::v2::PushPull>>,
        delay: Delay,
        usb_host: SAMDHost,
        midi_driver: MidiDriver,
        serial: UART0<
            Pad<SERCOM0, Pad3, atsamd_hal::gpio::Pin<PA07, atsamd_hal::gpio::v2::Alternate<atsamd_hal::gpio::v2::D>>>,
            Pad<SERCOM0, Pad2, atsamd_hal::gpio::Pin<PA06, atsamd_hal::gpio::v2::Alternate<atsamd_hal::gpio::v2::D>>>,
            (), ()>,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        let mut peripherals = cx.device;
        let mut core = cx.core;
        let mut clocks = GenericClockController::with_internal_32kosc(
            peripherals.GCLK,
            &mut peripherals.PM,
            &mut peripherals.SYSCTRL,
            &mut peripherals.NVMCTRL,
        );

        rtt_init_print!();

        log::set_max_level(LevelFilter::Debug);
        unsafe { log::set_logger_racy(&MY_LOGGER); }

        let mut pins = hal::Pins::new(peripherals.PORT);
        let mut red_led = pins.d13.into_open_drain_output(&mut pins.port);
        let mut delay = Delay::new(core.SYST, &mut clocks);

        let serial = hal::uart(
            &mut clocks,
            Hertz(115200),
            peripherals.SERCOM0,
            &mut peripherals.PM,
            pins.d3.into_floating_input(&mut pins.port),
            pins.d4.into_floating_input(&mut pins.port),
            &mut pins.port,
        );

        let usb_pins = Pins::new(
            pins.usb_dm.into_floating_input(&mut pins.port),
            pins.usb_dp.into_floating_input(&mut pins.port),
            Some(pins.usb_sof.into_floating_input(&mut pins.port)),
            Some(pins.usb_host_enable.into_floating_input(&mut pins.port)),
        );

        let mut usb_host = SAMDHost::new(
            peripherals.USB,
            usb_pins,
            &mut pins.port,
            &mut clocks,
            &mut peripherals.PM,
            &|| millis() as usize,
        );

        let mut midi_driver = MidiDriver::new();


        init::LateResources {
            delay,
            red_led,
            usb_host,
            midi_driver,
            serial,
        }
    }

    #[idle(resources = [red_led, delay])]
    fn idle(cx: idle::Context) -> ! {
        let red_led = cx.resources.red_led;
        let delay: &mut Delay = cx.resources.delay;

        // If we made it this far, things should be ok, so throttle the logging.
        // log::set_max_level(LevelFilter::Info);

        loop {
            delay.delay_ms(400u16);
            red_led.toggle();
            delay.delay_ms(400u16);
            red_led.toggle();
            info!("asdfasdfasdf");
        }
    }

    #[task(binds = USB, resources = [usb_host], spawn = [usb_task])]
    fn usb_irq(mut cx: usb_irq::Context) {
        let events = cx.resources.usb_host.lock(|u| u.handle_irq());
        cx.spawn.usb_task(events);
    }

    #[task(resources = [usb_host, midi_driver], priority = 3)]
    fn usb_task(mut cx: usb_task::Context, events: Events) {
        // TODO make driver list 'static
        let mut usb_drivers: Vec<&mut (dyn usb_host::Driver + Send + Sync), 16> = Vec::new();
        usb_drivers.push(cx.resources.midi_driver);
        cx.resources.usb_host.task(&mut usb_drivers, events)
    }

    extern "C" {
        // Reuse some interrupts for software task scheduling.
        fn SERCOM3();
        fn TC4();
    }
};
