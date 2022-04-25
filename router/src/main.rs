#![no_std]
#![no_main]

#![feature(slice_as_chunks)]
#![feature(alloc_error_handler)]

#[macro_use]
extern crate runtime;

#[macro_use]
extern crate bitfield;
#[macro_use]
extern crate cortex_m_rt;

extern crate cortex_m;
extern crate embedded_midi as midi;

use core::sync::atomic::AtomicU16;

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};

extern crate stm32f4xx_hal as hal;

// use ssd1306::{Builder, I2CDIBuilder};

// mod time;
mod devices;
mod apps;
// mod display;
mod route;
mod filter;
mod sysex;
mod port;

#[macro_use]
extern crate alloc;

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

pub type Handle = u16;

pub static NEXT_HANDLE: AtomicU16 = AtomicU16::new(0);


use crate::route::{Router, Service};
use crate::port::serial::SerialMidi;

use crate::apps::blinky_beat::BlinkyBeat;
use crate::apps::dw6000_control::Dw6000Control;

// use crate::display::gui::{self, Display};

use midi::{Receive, Transmit};
use usb_device::bus;

use midi::{CableNumber, Interface, Binding, channel, Note, PacketList};
use Binding::{Src};

// use ili9341::Ili9341;
// use display_interface_spi::SPIInterface;
//
// use embedded_graphics::pixelcolor::Rgb565;
// use embedded_hal::{
//     blocking::delay::{DelayMs, DelayUs},
//     digital::v2::OutputPin,
//     spi as espi,
// };

use hal::pac;

use hal::{
    // bring in .khz(), .mhz()
    time::U32Ext as _,
    serial,
    spi::{Spi, self},
    otg_fs::{USB, UsbBus, UsbBusType},
    gpio::{
        gpioa::*,
        gpiob::*,
        gpioc::PC13, GpioExt, Output,
        PushPull,
    },
    rcc::RccExt,
    prelude::*,
};

use alloc::string::String;
use core::alloc::Layout;
use core::ops::DerefMut;
use cortex_m::asm;
use hal::{interrupt};
use hal::gpio::AF7;
use runtime::allocator::CortexMSafeAlloc;
use runtime::{Local, Shared, spawn};
use crate::apps::bounce::Bounce;
use crate::devices::arturia::beatstep::beatstep_control_get;
use crate::pac::{CorePeripherals, Peripherals};

pub const CPU_FREQ: u32 = 96_000_000;

// pub const CYCLES_PER_MICROSEC: u32 = CPU_FREQ / 1_000_000;
// pub const CYCLES_PER_MILLISEC: u32 = CPU_FREQ / 1_000;

const IF_DW6000: Interface = Interface::Serial(0);
const IF_BEATSTEP: Interface = Interface::Serial(1);

static USB_EP_MEMORY: Local<[u32; 1024]> = Local::new("USB_EP_MEMORY", [0; 1024]);
static USB_BUS: Local<bus::UsbBusAllocator<UsbBusType>> = Local::uninit("USB_BUS");

static CHAOS: Shared<nanorand::WyRand> = Shared::uninit("RANDOM");

static ONBOARD_LED: Shared<PC13<Output<PushPull>>> = Shared::uninit("LED");

static MIDI_ROUTER: Shared<route::Router> = Shared::uninit("ROUTER");

static PORT_USB_MIDI: Shared<port::usb::UsbMidi> = Shared::uninit("USB_MIDI");
static PORT_BEATSTEP: Shared<SerialMidi<pac::USART1, (PB6<AF7>, PB7<AF7>)>> = Shared::uninit("UART1_MIDI");
static PORT_DW6000: Shared<SerialMidi<pac::USART2, (PA2<AF7>, PA3<AF7>)>> = Shared::uninit("UART2_MIDI");

// display: gui::Display<Ili9341<SPIInterface<Spi<hal::stm32::SPI1, (PA5<Alternate<hal::gpio::AF5>>, NoMiso, PA7<Alternate<hal::gpio::AF5>>)>, PB0<Output<PushPull>>, PA4<Output<PushPull>>>, PA6<Output<PushPull>>>, Rgb565>,

#[entry]
fn main() -> ! {
    let mut dev = Peripherals::take().unwrap();
    let mut core = CorePeripherals::take().unwrap();

    info!("Initializing");

    runtime::init();

    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();

    let rcc = dev.RCC.constrain();
    let clocks = rcc.cfgr
        .hclk(CPU_FREQ.Hz())
        .sysclk(CPU_FREQ.Hz())
        .pclk1(24.MHz())
        .pclk2(24.MHz())
        .freeze();

    let gpioa = dev.GPIOA.split();
    let gpiob = dev.GPIOB.split();
    let gpioc = dev.GPIOC.split();

    ONBOARD_LED.init_with(gpioc.pc13.into_push_pull_output());

    // Setup I2C Display
    // let scl = gpiob.pb8.into_alternate_af4().set_open_drain();
    // let sda = gpiob.pb9.into_alternate_af4().set_open_drain();
    //
    // let i2c = I2c::i2c1(dev.I2C1, (scl, sda), 400.khz(), clocks);
    // let interface = I2CDIBuilder::new().init(i2c);
    // let mut oled: GraphicsMode<_> = Builder::new().connect(interface).into();
    // oled.init().unwrap();
    //
    // display::draw_logo(&mut oled).unwrap();

    // let mut delay = CortexDelay {};

    // let sclk = gpioa.pa5.into_alternate_af5();
    // // let miso = gpioa.pa6.into_alternate_af5();
    // let mosi = gpioa.pa7.into_alternate_af5();
    //
    // let spi = spi::Spi::spi1(
    //     dev.SPI1,
    //     (sclk, NoMiso, mosi),
    //     espi::MODE_0,
    //     100.khz().into(),
    //     clocks,
    // );
    //
    // let lcd_cs = gpioa.pa4.into_push_pull_output();
    // let lcd_dc = gpiob.pb0.into_push_pull_output();
    //
    // let lcd_spi = SPIInterface::new(spi, lcd_dc, lcd_cs);
    //
    // let mut ts_cs = gpiob.pb1.into_push_pull_output();
    // ts_cs.set_high().expect("Could not disable touchscreen");
    //
    // let lcd_reset = gpioa.pa6.into_push_pull_output();
    // let ili9341 = Ili9341::new(lcd_spi, lcd_reset, &mut delay).expect("LCD init failed");

    // let display = Display::new(ili9341).unwrap();

    info!("Screen OK");

    let bs_tx = gpiob.pb6.into_alternate();
    let bs_rx = gpiob.pb7.into_alternate();
    let mut uart1 = dev.USART1.serial(
        (bs_tx, bs_rx),
        serial::config::Config::default()
            .baudrate(115200.bps()),
        &clocks,
    ).unwrap();
    uart1.listen(serial::Event::Rxne);

    PORT_BEATSTEP.init_with(SerialMidi::new(uart1, CableNumber::MIN));

    info!("BeatStep MIDI port OK");

    let dw_tx = gpioa.pa2.into_alternate();
    let dw_rx = gpioa.pa3.into_alternate();
    let mut uart2 = dev.USART2.serial(
        (dw_tx, dw_rx),
        serial::config::Config::default()
            .baudrate(31250.bps()),
        &clocks,
    ).unwrap();
    uart2.listen(serial::Event::Rxne);

    PORT_DW6000.init_with(SerialMidi::new(uart2, CableNumber::MIN));

    info!("DW6000 MIDI port OK");

    let usb = USB {
        usb_global: dev.OTG_FS_GLOBAL,
        usb_device: dev.OTG_FS_DEVICE,
        usb_pwrclk: dev.OTG_FS_PWRCLK,
        pin_dm: gpioa.pa11.into_alternate(),
        pin_dp: gpioa.pa12.into_alternate(),
        hclk: clocks.hclk(),
    };

    USB_BUS.init_with(UsbBus::new(usb, USB_EP_MEMORY.raw_mut()));

    let midi_class = port::usb::MidiClass::new(&USB_BUS);
    // USB devices init _after_ classes
    let usb_dev = port::usb::usb_device(&USB_BUS);

    info!("USB dev OK");

    PORT_USB_MIDI.init_with(port::usb::UsbMidi {
        dev: usb_dev,
        midi_class,
    });

    // let chaos = nanorand::WyRand::new_seed(0);
    // info!("Chaos OK");

    MIDI_ROUTER.init_with(Router::default());
    info!("Router OK");

    // let _usb_echo = midi_router.add_route(
    //     Route::echo(Interface::USB(0))
    //         .filter(|_now, cx| print_message(cx)));

    // let _serial_print = midi_router
    //     .add_route(route::Route::from(DW6000)
    //         .filter(|cx| filter::print_message(cx)));

    // let _usb_print = midi_router.add_route(
    //     Route::to(Interface::USB(0))
    //         .filter(|_now, cx| print_packets(cx)));

    // let _usb_print_in = midi_router.add_route(
    //     Route::from(Interface::USB(0))
    //         .filter(|_now, cx| print_packets(cx)));

    // let _evo_match = midi_router.bind(
    //     Route::from(Interface::Serial(0))
    //         .filter(SysexCapture(dsi_evolver::program_parameter_matcher()))
    //         .filter(PrintTags)
    // );

    // let _evo_match = midi_router.bind(
    //     Route::from(Interface::Serial(0))
    //         .filter(capture_sysex(dsi_evolver::program_parameter_matcher()))
    //         .filter(print_tag())
    // );
    // let _bstep_2_dw = midi_router.bind(Route::link(Interface::USB, Interface::Serial(0)));

    // let mut dwctrl = Dw6000Control::new((IF_DW6000, channel(1)), (IF_BEATSTEP, channel(1)));
    // dwctrl.start().unwrap();

    let mut bbeat = BlinkyBeat::new((IF_BEATSTEP, channel(1)), vec![Note::C1m, Note::Cs1m, Note::B1m, Note::G0]);
    bbeat.start().unwrap();

    // let mut bounce = Bounce::new();
    // bounce.start().unwrap();

    runtime::spawn(async {
        loop {
            let mut led = ONBOARD_LED.lock().await;
            led.toggle();
            if runtime::delay_ms(2000).await.is_err() { break; }
        }
    });

    unsafe {
        core.NVIC.set_priority(pac::Interrupt::USART1, 3);
        pac::NVIC::unmask(pac::Interrupt::USART1);

        core.NVIC.set_priority(pac::Interrupt::USART2, 3);
        pac::NVIC::unmask(pac::Interrupt::USART2);

        core.NVIC.set_priority(pac::Interrupt::OTG_FS_WKUP, 3);
        pac::NVIC::unmask(pac::Interrupt::OTG_FS_WKUP);

        // core.NVIC.set_priority(pac::Interrupt::OTG_FS, 3);
        // pac::NVIC::unmask(pac::Interrupt::OTG_FS);
    }

    loop {
        // // wake up
        runtime::run_scheduled();
        // // do things
        runtime::process_queue();
        // breathe?
        // cortex_m::asm::delay(400);
    }
}

// #[idle(shared = [on_board_led])]
// fn idle(mut cx: idle::Context) -> ! {
//
// }

/// USB polling required every 0.125 millisecond
// #[task(binds = OTG_FS_WKUP, shared = [usb_midi], priority = 3)]
#[interrupt]
unsafe fn OTG_FS_WKUP() {
    spawn(async {
        let mut usb = PORT_USB_MIDI.lock().await;
        usb.poll();
    })
}

/// USB receive interrupt
/// Using LOWER priority to backoff on USB reception if Serial queues not emptying fast enough
// #[task(binds = OTG_FS, shared = [usb_midi], priority = 3)]
#[interrupt]
unsafe fn OTG_FS() {
    // poll() is also required here else receive may block forever
    spawn(async {
        let mut usb_midi = PORT_USB_MIDI.lock().await;
        if usb_midi.poll() {
            while let Some(packet) = usb_midi.receive().unwrap() {
                midi_route(Src(IF_BEATSTEP), PacketList::single(packet)).await;
            }
        }
    })
}

/// Serial receive interrupt
// #[task(binds = USART1, shared = [beatstep], priority = 3)]
#[interrupt]
unsafe fn USART1() {
    spawn(async {
        loop {
            let mut bstep = PORT_BEATSTEP.lock().await;
            bstep.flush().unwrap();
            match bstep.receive() {
                Ok(Some(packet)) => {
                    debug!("MIDI from beatstep {:?}", packet);
                    midi_route(Src(IF_BEATSTEP), PacketList::single(packet)).await;
                    continue;
                }
                Err(e) => {
                    warn!("Error serial read {:?}", e);
                    break;
                }
                _ => { break; }
            }
        }
    });
}

/// Serial receive interrupt
// #[task(binds = USART2, shared = [dw6000], priority = 3)]
#[interrupt]
unsafe fn USART2() {
    spawn(async {
        let mut dw6000 = PORT_DW6000.lock().await;
        if let Err(err) = dw6000.flush() {
            warn!("Serial flush failed {:?}", err);
        }

        while let Ok(Some(packet)) = dw6000.receive() {
            midi_route(Src(IF_DW6000), PacketList::single(packet)).await;
        }
    });
}


// #[task(shared = [midi_router, tasks], priority = 2, capacity = 16)]
async fn midi_route(binding: Binding, packets: PacketList) {
    let mut router = MIDI_ROUTER.lock().await;
    if let Err(e) = router.midi_route(packets, binding).await {
        warn!("MIDI Routing error {}", e);
    }
}

// TODO split output queues (one task per interface)
// #[task(shared = [usb_midi, dw6000, beatstep], capacity = 128, priority = 2)]
async fn midi_send(interface: Interface, packets: PacketList) {
    match interface {
        // includes BeatStep
        Interface::USB(_) => {
            let mut midi = PORT_USB_MIDI.lock().await;
            if let Err(e) = midi.transmit(packets) {
                info!("Failed to send USB MIDI: {:?}", e)
            }
        }

        IF_DW6000 => {
            let mut dw6000 = PORT_DW6000.lock().await;
            if let Err(e) = dw6000.transmit(packets) {
                info!("Failed to send Serial MIDI: {:?}", e)
            }
        }

        IF_BEATSTEP => {
            let mut bstep = PORT_BEATSTEP.lock().await;
            if let Err(e) = bstep.transmit(packets) {
                info!("Failed to send Serial MIDI: {:?}", e)
            }
        }

        _ => {}
    }
}

// Update the UI - using
// #[task(/*local = [display],*/ capacity = 8)]
fn midisplay(_text: String) {
    // cx.local.display.print(text).unwrap();
}



