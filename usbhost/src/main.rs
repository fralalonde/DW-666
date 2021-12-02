#![feature(async_closure)]
#![feature(const_raw_ptr_deref)]
#![feature(const_fn_trait_bound)]

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
const HEAP_SIZE: usize = 8 * 1024;
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

use trinket_m0 as bsp;

use bsp::clock::GenericClockController;
use bsp::entry;
use bsp::pac::{interrupt, CorePeripherals, Peripherals, TC4, TC5};

use cortex_m::peripheral::NVIC;

use trinket_m0::clock::{ClockGenId, ClockSource};
use trinket_m0::hal::hal::timer::CountDown;
use trinket_m0::hal::timer::TimerCounter;
use trinket_m0::hal::timer_traits::InterruptDrivenTimer;
use trinket_m0::time::U32Ext;

use core::mem;

use atsamd_hal as hal;
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

use atsamd_hal::time::{Hertz};

use atsamd_hal::gpio::v2::*;

use atsamd_hal::sercom::UART0;

use atsamd_usb_host::{SAMDHost, Pins, HostEvent, DriverEvent};

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
use atsamd_usb_host::usb_host::{Driver, DriverError};
use embedded_time::Clock;
use embedded_time::duration::{Microseconds, Milliseconds};
use heapless::Vec;
use atsamd_usb_host::HostEvent::NoEvent;
use sync_thumbv6m::spin::SpinMutex;

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

static LOGGER: RTTLogger = RTTLogger {};

const UPSTREAM_SERIAL: Interface = Interface::Serial(0);

static mut USB_HOST: MaybeUninit<SAMDHost> = mem::MaybeUninit::uninit();

static mut USB_DRIVERS: Vec<Arc<SpinMutex<(dyn Driver + Send + Sync)>>, 4> = heapless::Vec::new();


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

    rtt_init_print!(NoBlockTrim, 1024);
    info!("init");

    let _gclk = clocks.gclk0();
    // let rtc_clock_src = clocks
    //     .configure_gclk_divider_and_source(ClockGenId::GCLK2, 1, ClockSource::OSC32K, false)
    //     .unwrap();
    // clocks.configure_standby(ClockGenId::GCLK2, true);
    // let rtc_clock = clocks.rtc(&rtc_clock_src).unwrap();
    // let rtc = Rtc::count32_mode(peripherals.RTC, rtc_clock.freq(), &mut peripherals.PM);

    log::set_max_level(LevelFilter::Trace);
    unsafe { log::set_logger_racy(&LOGGER).unwrap(); }

    let mut pins = bsp::Pins::new(peripherals.PORT);
    let mut red_led = pins.d13.into_open_drain_output(&mut pins.port);
    let mut delay = Delay::new(core.SYST, &mut clocks);

    runtime::init();

    let timer_clock = clocks
        .configure_gclk_divider_and_source(ClockGenId::GCLK4, 1, ClockSource::OSC32K, false)
        .unwrap();
    let tc45 = &clocks.tc4_tc5(&timer_clock).unwrap();

    // let mut tc4 = TimerCounter::tc4_(tc45, peripherals.TC4, &mut peripherals.PM);
    // tc4.start(800.hz());
    // tc4.enable_interrupt();

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
        runtime::now_millis,
    );
    info!("USB Host OK");

    unsafe {
        USB_DRIVERS.push(Arc::new(SpinMutex::new(MidiDriver::default())));
        usb_host.reset();
        USB_HOST = MaybeUninit::new(usb_host);
    };

    info!("Board Initialization Complete");

    unsafe {
        core.NVIC.set_priority(interrupt::USB, 3);
        NVIC::unmask(interrupt::USB);

        core.NVIC.set_priority(interrupt::SERCOM0, 3);
        NVIC::unmask(interrupt::SERCOM0);
    }

    runtime::spawn(async {
        loop {
            usb_service(NoEvent).await;
            runtime::delay_us(125).await;
        }
    });

    loop {
        // wake up
        runtime::run_scheduled();
        // do things
        runtime::process_queue();
        // breathe
        cortex_m::asm::delay(4096);
    }
}

fn midispatch(binding: Binding, packets: PacketList) {
    // let router: &mut route::Router = cx.resources.midi_router;
    // router.midispatch(cx.scheduled, packets, binding, cx.spawn).unwrap();
}

// #[interrupt]
// fn TC4() {
//     exec::spawn(usb_service(HostEvent::NoEvent));
//     unsafe { TC4::ptr().as_ref().unwrap().count16().intflag.modify(|_, w| w.ovf().set_bit()); }
// }

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
    let host_event = USB_HOST.assume_init_ref().get_host_event();
    runtime::spawn(usb_service(host_event))
}

async fn usb_service(host_event: HostEvent) {
    let driver_event = unsafe { USB_HOST.assume_init_mut().tick(host_event).await };

    if let Some(driver_event) = driver_event {
        match driver_event {
            DriverEvent::Tick => {
                for mut driver in unsafe { &USB_DRIVERS } {
                    let mut driver = driver.lock();
                    if let Err(e) = driver.tick(runtime::now_millis, unsafe { USB_HOST.assume_init_mut() }) {
                        warn!("USB Driver Error [{:?}]: {:?}", driver, e);
                        if let DriverError::Permanent(addr, _) = e {
                            driver.remove_device(addr);
                        }
                    }
                }
            }
            DriverEvent::NewDevice(desc, addr) => {
                let mut device_registered = false;
                for driver in unsafe { &mut USB_DRIVERS } {
                    let mut driver = driver.lock();
                    if driver.want_device(&desc) {
                        if let Err(e) = driver.add_device(desc, addr) {
                            error!("Driver failed to add device");
                        } else {
                            trace!("USB Device registered by driver");
                            device_registered = true;
                            break;
                        }
                    }
                }
                if !device_registered {
                    warn!("USB Device not registered by drivers");
                }
            }
        }
    }
}

