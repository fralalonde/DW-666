#![no_std]
#![no_main]

#[macro_use]
extern crate log;

#[macro_use]
extern crate rtt_target;

use panic_rtt_target as _;

use trinket_m0 as hal;

use hal::clock::GenericClockController;
use hal::pac::{ CorePeripherals, Peripherals};

use atsamd_hal::time::Hertz;

use atsamd_hal::gpio::v2::PA10;
use atsamd_hal::delay::Delay;
use atsamd_hal::hal::blocking::delay::DelayMs;
use atsamd_hal::sercom::UART0;

use atsamd_usb_host::{SAMDHost, Pins, Events, Event};

use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::Relaxed;

use heapless::Vec;
use crate::midi_driver::MidiDriver;
use log::LevelFilter;

use rtt_target::*;
use hal::sercom::*;
use atsamd_hal::gpio::{self, *};

use minimidi::{CableNumber, Interface, PacketList, Binding, Receive, };
use crate::port::serial::SerialMidi;

mod port;
mod midi_driver;

static mut MILLIS: AtomicUsize = AtomicUsize::new(0);

fn millis() -> usize {
    // FIXME this will rollover quickly...
    unsafe { MILLIS.load(Relaxed) }
}

use log::{Metadata, Record};
use atsamd_hal::gpio::PfD;
use minimidi::Binding::Src;

/// An RTT-based logger implementation.
pub struct RTTLogger {}

impl log::Log for RTTLogger     {
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

const UPSTREAM_SERIAL: Interface = Interface::Serial(0);

#[rtic::app(device = crate::hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        red_led: atsamd_hal::gpio::Pin<PA10, atsamd_hal::gpio::v2::Output<gpio::v2::PushPull>>,
        delay: Delay,
        usb_host: SAMDHost,
        midi_driver: MidiDriver,
        serial_midi: SerialMidi<UART0<Sercom0Pad3<Pa7<PfD>>, Sercom0Pad2<Pa6<PfD>>, (), ()>>,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        // RTIC needs statics to go first
        static MY_LOGGER: RTTLogger = RTTLogger {};

        let mut peripherals: Peripherals = cx.device;
        let core: CorePeripherals = cx.core;
        let mut clocks = GenericClockController::with_internal_32kosc(
            peripherals.GCLK,
            &mut peripherals.PM,
            &mut peripherals.SYSCTRL,
            &mut peripherals.NVMCTRL,
        );

        rtt_init_print!();
        info!("init");

        log::set_max_level(LevelFilter::Trace);
        unsafe { log::set_logger_racy(&MY_LOGGER).unwrap(); }

        let mut pins = hal::Pins::new(peripherals.PORT);
        let red_led = pins.d13.into_open_drain_output(&mut pins.port);
        let delay = Delay::new(core.SYST, &mut clocks);

        let serial: UART0<Sercom0Pad3<Pa7<PfD>>, Sercom0Pad2<Pa6<PfD>>, (), ()> = hal::uart(
            &mut clocks,
            Hertz(115200),
            peripherals.SERCOM0,
            &mut peripherals.PM,
            pins.d3.into_floating_input(&mut pins.port),
            pins.d4.into_floating_input(&mut pins.port),
            &mut pins.port,
        );
        let serial_midi = port::serial::SerialMidi::new(serial, CableNumber::MIN);
        info!("Serial OK");

        let usb_pins = Pins::new(
            pins.usb_dm.into_floating_input(&mut pins.port),
            pins.usb_dp.into_floating_input(&mut pins.port),
            Some(pins.usb_sof.into_floating_input(&mut pins.port)),
            Some(pins.usb_host_enable.into_floating_input(&mut pins.port)),
        );
        info!("USB OK");

        let mut usb_host = SAMDHost::new(
            peripherals.USB,
            usb_pins,
            &mut pins.port,
            &mut clocks,
            &mut peripherals.PM,
            &|| millis() as usize,
        );
        info!("USB Host OK");

        let midi_driver = MidiDriver::default();
        info!("USB MIDI driver created");

        // enable USB
        usb_host.reset_periph();

        info!("Board Initialization Complete");

        let mut usb_drivers: Vec<&mut (dyn usb_host::Driver + Send + Sync), 16> = Vec::new();
        // usb_drivers.push(cx.resources.midi_driver);
        usb_host.process_event(&mut usb_drivers, Event::Detached);

        init::LateResources {
            delay,
            red_led,
            usb_host,
            midi_driver,
            serial_midi,
        }
    }

    #[idle(resources = [red_led, delay, usb_host], spawn = [usb_task])]
    fn idle(cx: idle::Context) -> ! {
        let red_led = cx.resources.red_led;
        let delay: &mut Delay = cx.resources.delay;
        info!("Idle Loop Start");

        loop {
            delay.delay_ms(250u16);
            red_led.toggle();
        }
    }

    /// Serial receive interrupt
    #[task(binds = SERCOM0, spawn = [midispatch], resources = [serial_midi], priority = 3)]
    fn serial_irq(cx: serial_irq::Context) {
        info!("Serial IRQ");
        if let Err(err) = cx.resources.serial_midi.flush() {
            error!("Serial flush failed {:?}", err);
        }

        while let Ok(Some(packet)) = cx.resources.serial_midi.receive() {
            cx.spawn.midispatch(Src(UPSTREAM_SERIAL), PacketList::single(packet)).unwrap();
        }
    }

    #[task(binds = USB, resources = [usb_host], spawn = [usb_task], priority = 3)]
    fn usb_irq(cx: usb_irq::Context) {
        let events = cx.resources.usb_host.handle_irq();
        for event in events.iter().filter_map(|z| *z) {
            cx.spawn.usb_task(event)/*.unwrap()*/;
        }
    }

    #[task(resources = [usb_host, midi_driver], priority = 2, capacity = 16)]
    fn usb_task(mut cx: usb_task::Context, event: Event) {
        // TODO make driver list 'static
        debug!("pwoceesdf");
        let mut usb_drivers: Vec<&mut (dyn usb_host::Driver + Send + Sync), 4> = Vec::new();
        usb_drivers.push(cx.resources.midi_driver);
        cx.resources.usb_host.lock(|h| h.process_event(&mut usb_drivers, event));
    }

    #[task(/*spawn = [midisend, midisplay],*/ /*resources = [midi_router, tasks],*/ priority = 2, capacity = 16)]
    fn midispatch(cx: midispatch::Context, binding: Binding, packets: PacketList) {
        // let router: &mut route::Router = cx.resources.midi_router;
        // router.midispatch(cx.scheduled, packets, binding, cx.spawn).unwrap();
    }


    extern "C" {
        // Reuse some interrupts for software task scheduling.
        fn SERCOM3();
        fn TC4();
    }
};
