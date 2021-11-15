#![feature(async_closure)]

#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate log;

extern crate rtt_target;

#[macro_use]
extern crate async_trait;

extern crate panic_rtt_target as _;

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};
use embed_alloc::CortexMSafeAlloc;

const FAST_HEAP_SIZE: usize = 16 * 1024;
const HEAP_SIZE: usize = 48 * 1024;
const LEAF_SIZE: usize = 16;

pub static mut FAST_HEAP: [u8; FAST_HEAP_SIZE] = [0u8; FAST_HEAP_SIZE];
pub static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

#[cfg_attr(not(test), global_allocator)]
static ALLOC: CortexMSafeAlloc = unsafe {
    let fast_param = FastAllocParam::new(FAST_HEAP.as_ptr(), FAST_HEAP_SIZE);
    let buddy_param = BuddyAllocParam::new(HEAP.as_ptr(), HEAP_SIZE, LEAF_SIZE);
    CortexMSafeAlloc(NonThreadsafeAlloc::new(fast_param, buddy_param))
};

mod port;
mod midi_driver;
mod exec;

use trinket_m0 as bsp;

use bsp::clock::GenericClockController;
use bsp::entry;
use bsp::pac::{interrupt, CorePeripherals, Peripherals, TC4};

use cortex_m::peripheral::NVIC;

use trinket_m0::clock::{ClockGenId, ClockSource};
use trinket_m0::hal::hal::timer::CountDown;
use trinket_m0::hal::timer::TimerCounter;
use trinket_m0::hal::timer_traits::InterruptDrivenTimer;
use trinket_m0::time::U32Ext;

use alloc::boxed::Box;

use core::mem;

use atsamd_hal as hal;
use embedded_hal as ehal;
use hal::pac;

use hal::sercom::{
    v2::{
        uart::{self, BaudMode, Oversampling},
        Sercom0,
        Sercom2,
    },
    I2CMaster3,
    I2CMaster2,
    I2CMaster1,
    I2CMaster0,
};

use panic_rtt_target as _;

use hal::delay::Delay;

use atsamd_hal::pac::{USB};
use atsamd_hal::time::{Hertz};

use atsamd_hal::gpio::v2::*;

use atsamd_hal::sercom::UART0;

use atsamd_usb_host::{SAMDHost, Pins, Event};

use crate::midi_driver::MidiDriver;
use log::LevelFilter;

use rtt_target::*;
use hal::sercom::*;
use atsamd_hal::gpio::{self, *};

use minimidi::{CableNumber, Interface, PacketList, Binding, Receive};
use crate::port::serial::SerialMidi;

use log::{Metadata, Record};
use atsamd_hal::gpio::PfD;
use minimidi::Binding::Src;

use bsp::rtc::{Rtc};

use core::mem::{MaybeUninit};
use core::sync::atomic::Ordering::Relaxed;
use atomic_polyfill::AtomicUsize;

use sync_thumbv6m::alloc::Arc;
use atsamd_usb_host::usb_host::Driver;
use crate::exec::Instant;

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

static _LOGGER: RTTLogger = RTTLogger {};

const UPSTREAM_SERIAL: Interface = Interface::Serial(0);

static mut USB_HOST: MaybeUninit<SAMDHost> = mem::MaybeUninit::uninit();

static mut REACTOR: MaybeUninit<Arc<exec::Reactor>> = mem::MaybeUninit::uninit();

static MILLIS: AtomicUsize = AtomicUsize::new(0);

fn millis() -> Instant {
    MILLIS.load(Relaxed)
}

#[entry]
fn main() -> ! {
    let mut peripherals = Peripherals::take().unwrap();
    let mut core = CorePeripherals::take().unwrap();

    // internal 32khz required for USB to complete swrst
    let mut clocks = GenericClockController::with_internal_32kosc(
        peripherals.GCLK,
        &mut peripherals.PM,
        &mut peripherals.SYSCTRL,
        &mut peripherals.NVMCTRL,
    );

    rtt_init_print!();
    info!("init");

    let _gclk = clocks.gclk0();
    let rtc_clock_src = clocks
        .configure_gclk_divider_and_source(ClockGenId::GCLK2, 1, ClockSource::OSC32K, false)
        .unwrap();
    clocks.configure_standby(ClockGenId::GCLK2, true);
    let rtc_clock = clocks.rtc(&rtc_clock_src).unwrap();
    let rtc = Rtc::count32_mode(peripherals.RTC, rtc_clock.freq(), &mut peripherals.PM);

    log::set_max_level(LevelFilter::Trace);
    unsafe { log::set_logger_racy(&_LOGGER).unwrap(); }

    let mut pins = bsp::Pins::new(peripherals.PORT);
    let mut red_led = pins.d13.into_open_drain_output(&mut pins.port);
    let mut delay = Delay::new(core.SYST, &mut clocks);

    unsafe { REACTOR = MaybeUninit::new(Arc::new(exec::Reactor::new(16, millis))) };

    let timer_clock = clocks
        .configure_gclk_divider_and_source(ClockGenId::GCLK4, 1, ClockSource::OSC32K, false)
        .unwrap();
    let tc45 = &clocks.tc4_tc5(&timer_clock).unwrap();

    let mut tc4 = TimerCounter::tc4_(tc45, peripherals.TC4, &mut peripherals.PM);
    tc4.start(1.khz());
    tc4.enable_interrupt();

    let serial: UART0<Sercom0Pad3<Pa7<PfD>>, Sercom0Pad2<Pa6<PfD>>, (), ()> = bsp::uart(
        &mut clocks,
        Hertz(115200),
        peripherals.SERCOM0,
        &mut peripherals.PM,
        pins.d3.into_floating_input(&mut pins.port),
        pins.d4.into_floating_input(&mut pins.port),
        &mut pins.port,
    );
    let serial_midi = crate::port::serial::SerialMidi::new(serial, CableNumber::MIN);
    info!("Serial OK");

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
        Box::new(MidiDriver::default()),
        millis,
    );
    info!("USB Host OK");

    let midi_driver = MidiDriver::default();
    info!("USB MIDI driver created");

    // enable USB
    usb_host.reset();

    info!("Board Initialization Complete");

    unsafe {
        core.NVIC.set_priority(interrupt::USB, 3);
        NVIC::unmask(interrupt::USB);

        core.NVIC.set_priority(interrupt::TC4, 3);
        NVIC::unmask(interrupt::TC4);

        core.NVIC.set_priority(interrupt::SERCOM0, 3);
        NVIC::unmask(interrupt::SERCOM0);
    }

    // Flash the LED in a spin loop to demonstrate that USB is
    // entirely interrupt driven.
    loop {
        info!("Idle Loop Start");
        //
        // delay.delay_ms(255u16);
        // red_led.toggle();

        unsafe { REACTOR.assume_init_ref().advance() };
    }
}

fn midispatch(binding: Binding, packets: PacketList) {
    // let router: &mut route::Router = cx.resources.midi_router;
    // router.midispatch(cx.scheduled, packets, binding, cx.spawn).unwrap();
}

#[interrupt]
fn TC4() {
    trace!("IRQ TC4");
    MILLIS.fetch_add(1, Relaxed);
    // TODO check if next scheduled delay awakes
    unsafe {
        TC4::ptr().as_ref().unwrap().count16().intflag.modify(|_, w| w.ovf().set_bit());
    }
}

#[interrupt]
fn SERCOM0() {
    trace!("IRQ SERCOM0");
    // if let Err(err) = cx.shared.serial_midi.lock(|m| m.flush()) {
    //     error!("Serial flush failed {:?}", err);
    // }
    //
    // while let Ok(Some(packet)) = cx.shared.serial_midi.lock(|m| m.receive()) {
    //     midispatch::spawn(Src(UPSTREAM_SERIAL), PacketList::single(packet)).unwrap();
    // }
}

#[interrupt]
unsafe fn USB() {
    trace!("IRQ USB");
    let event = USB_HOST.assume_init_ref().irq_next_event();
    REACTOR.assume_init_ref().clone().spawn(
        USB_HOST.assume_init_mut().tick(event)
    )
}

