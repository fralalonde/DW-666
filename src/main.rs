#![no_std]
#![no_main]

#![feature(slice_as_chunks)]
#![feature(alloc_error_handler)]
#![feature(async_closure)]
#![feature(async_fn_in_trait)]

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

mod devices;
mod apps;
// mod display;

mod filter;
mod sysex;
mod port;

#[macro_use]
extern crate alloc;

// 16k fast Heap
const FAST_HEAP_SIZE: usize = 16 * 1024;

// 32k slow heap
const HEAP_SIZE: usize = 32 * 1024;

// 16 bytes leaf
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

use crate::port::serial::{SerialMidi};

// use crate::display::gui::{self, Display};

use midi::{MidiError, Receive, Transmit};
use usb_device::bus;

use midi::{MidiInterface, MidiBinding, channel, Note, PacketList};

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

use runtime::{ExtU32};

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

use hal::{interrupt};

use runtime::allocator::CortexMSafeAlloc;
use runtime::{Local, Shared, spawn};
use crate::apps::{blinky_beat, bounce, dw6_control};

use crate::filter::{print_message, print_packets};
use crate::pac::{CorePeripherals, Peripherals};

pub const CPU_FREQ: u32 = 96_000_000;

static CORE: Local<CorePeripherals> = Local::uninit("CORE");

static USB_EP_MEMORY: Local<[u32; 1024]> = Local::uninit("USB_EP_MEMORY");
static USB_BUS: Local<bus::UsbBusAllocator<UsbBusType>> = Local::uninit("USB_BUS");

static CHAOS: Shared<nanorand::WyRand> = Shared::uninit("RANDOM");

static ONBOARD_LED: Shared<PC13<Output<PushPull>>> = Shared::uninit("LED");

static MIDI_USB_1_RX: Local<fn(PacketList)> = Local::uninit("MIDI_USB_1_RX");
static MIDI_DIN_1_RX: Local<fn(PacketList)> = Local::uninit("MIDI_DIN_1_RX");
static MIDI_DIN_2_RX: Local<fn(PacketList)> = Local::uninit("MIDI_DIN_2_RX");

static MIDI_USB_1_PORT: Local<port::usb::UsbMidi> = Local::uninit("MIDI_USB_1_PORT");
static MIDI_DIN_1_PORT: Local<SerialMidi<pac::USART1>> = Local::uninit("MIDI_DIN_1_PORT");
static MIDI_DIN_2_PORT: Local<SerialMidi<pac::USART2>> = Local::uninit("MIDI_DIN_2_PORT");

// display: gui::Display<Ili9341<SPIInterface<Spi<hal::stm32::SPI1, (PA5<Alternate<hal::gpio::AF5>>, NoMiso, PA7<Alternate<hal::gpio::AF5>>)>, PB0<Output<PushPull>>, PA4<Output<PushPull>>>, PA6<Output<PushPull>>>, Rgb565>,

#[entry]
fn main() -> ! {
    let dev = Peripherals::take().unwrap();
    let core = CORE.init_static(CorePeripherals::take().unwrap());

    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();

    let rcc = dev.RCC.constrain();
    let clocks = rcc.cfgr
        .hclk(CPU_FREQ.Hz())
        .sysclk(CPU_FREQ.Hz())
        .pclk1(24.MHz())
        .pclk2(24.MHz())
        .freeze();

    runtime::init(&mut core.SYST);

    let gpioa = dev.GPIOA.split();
    let gpiob = dev.GPIOB.split();
    let gpioc = dev.GPIOC.split();

    ONBOARD_LED.init_static(gpioc.pc13.into_push_pull_output());

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

    let bs_tx = gpiob.pb6;
    let bs_rx = gpiob.pb7;
    let mut uart1 = dev.USART1.serial(
        (bs_tx, bs_rx),
        serial::config::Config::default()
            // high-speed MIDI coprocessor link
            // https://github.com/gdsports/usbhostcopro
            .baudrate(115200.bps()),
        &clocks,
    ).unwrap();
    uart1.listen(serial::Event::Rxne);
    MIDI_DIN_1_PORT.init_static(SerialMidi::new(uart1));
    info!("OK: SerialMidi 1");

    let dw_tx = gpioa.pa2;
    let dw_rx = gpioa.pa3;
    let mut uart2 = dev.USART2.serial(
        (dw_tx, dw_rx),
        serial::config::Config::default()
            .baudrate(31250.bps()),
        &clocks,
    ).unwrap();
    uart2.listen(serial::Event::Rxne);
    MIDI_DIN_2_PORT.init_static(SerialMidi::new(uart2));
    info!("OK: SerialMidi 2");

    let usb = USB::new(
        (dev.OTG_FS_GLOBAL, dev.OTG_FS_DEVICE, dev.OTG_FS_PWRCLK),
        (gpioa.pa11, gpioa.pa12),
        &clocks,
    );

    let ep_memory = USB_EP_MEMORY.init_static([0; 1024]);
    USB_BUS.init_static(UsbBus::new(usb, ep_memory));

    let midi_class = port::usb::MidiClass::new(&USB_BUS);
    // USB devices init _after_ classes
    let usb_dev = port::usb::usb_device(&USB_BUS);

    info!("OK: USB MIDI Device");

    MIDI_USB_1_PORT.init_static(port::usb::UsbMidi {
        dev: usb_dev,
        midi_class,
    });

    let chaos = nanorand::WyRand::new_seed(0);
    info!("OK: Chaos");

    /*    let mut midi_router = Router::default();

        let _usb_echo = midi_router
            .echo(MidiInterface::USB(0))
            .filter(|cx| print_message(cx))
            .add();

        let _serial_print = midi_router
            .from(IF_DW6000)
            .filter(|cx| print_message(cx))
            .add();

        let _usb_print = midi_router
            .to(MidiInterface::USB(0))
            .filter(|cx| print_packets(cx))
            .add();

        let _usb_print_in = midi_router
            .from(MidiInterface::USB(0))
            .filter(|cx| print_packets(cx))
            .add();*/

    // let _evo_match = midi_router
    //     .from(MidiInterface::Serial(0))
    //     .filter(SysexCapture(dsi_evolver::program_parameter_matcher()))
    //     .filter(PrintTags)
    //     .add();
    //
    // let _evo_match = midi_router
    //     .from(MidiInterface::Serial(0))
    //     .filter(capture_sysex(dsi_evolver::program_parameter_matcher()))
    //     .filter(print_tag())
    //     .add();

    // let _bstep_2_dw = midi_router
    //     .link(MidiInterface::USB(0), MidiInterface::Serial(0))
    //     .add();

    // let mut init_ctx = InitContext {
    //     din_1_dispatch: PacketDispatch::new(MidiInterface::Serial(0)),
    //     din_2_dispatch: PacketDispatch::new(MidiInterface::Serial(1)),
    //     usb_1_dispatch: PacketDispatch::new(MidiInterface::USB(0)),
    // };

    info!("Router OK");

    dw6_control::start_app();
    bounce::start_app();
    blinky_beat::start_app(channel(1), &[Note::C1m, Note::Cs1m, Note::B1m, Note::G0]);

    spawn(async {
        loop {
            let mut led = ONBOARD_LED.lock().await;
            led.toggle();
            info!("BLINK");
            if runtime::delay(500.millis()).await.is_err() { break; }
        }
    });

    unsafe {
        core.NVIC.set_priority(pac::Interrupt::USART1, 3);
        pac::NVIC::unmask(pac::Interrupt::USART1);

        core.NVIC.set_priority(pac::Interrupt::USART2, 3);
        pac::NVIC::unmask(pac::Interrupt::USART2);

        core.NVIC.set_priority(pac::Interrupt::OTG_FS_WKUP, 3);
        pac::NVIC::unmask(pac::Interrupt::OTG_FS_WKUP);

        core.NVIC.set_priority(pac::Interrupt::OTG_FS, 3);
        pac::NVIC::unmask(pac::Interrupt::OTG_FS);
    }

    loop {
        // wake up
        runtime::run_scheduled();
        // do things
        runtime::process_queue();
        // breathe?
        // TODO sleep until next scheduled task
        cortex_m::asm::delay(256);
    }
}

/// USB polling required every 0.125 millisecond
// #[task(binds = OTG_FS_WKUP, shared = [usb_midi], priority = 3)]
#[interrupt]
unsafe fn OTG_FS_WKUP() {
    pac::NVIC::mask(pac::Interrupt::OTG_FS_WKUP);

    // let cs = unsafe { critical_section::CriticalSection::new() };
    let mut usb = unsafe { MIDI_USB_1_PORT.raw_mut() };
    usb.poll();

    pac::NVIC::unmask(pac::Interrupt::OTG_FS_WKUP);
}

/// USB receive interrupt
/// Using LOWER priority to backoff on USB reception if Serial queues not emptying fast enough
#[interrupt]
unsafe fn OTG_FS() {
    pac::NVIC::mask(pac::Interrupt::OTG_FS);
    // poll() is also required here else receive may block forever
    let mut usb = unsafe { MIDI_USB_1_PORT.raw_mut() };
    if usb.poll() {
        while let Some(packet) = usb.receive().unwrap() {
            // TODO passthru?
        }
    }
    pac::NVIC::unmask(pac::Interrupt::OTG_FS);
}

#[interrupt]
unsafe fn USART1() {
    pac::NVIC::mask(pac::Interrupt::USART1);

    let bstep = unsafe { MIDI_DIN_1_PORT.raw_mut() };

    bstep.flush().unwrap();
    loop {
        match bstep.receive() {
            Ok(Some(packet)) => {
                debug!("MIDI from beatstep {:?}", packet);
                (MIDI_DIN_1_RX)(PacketList::single(packet));
                continue;
            }
            Err(e) => {
                warn!("Error serial read {:?}", e);
                break;
            }
            _ => { break; }
        }
    }
    pac::NVIC::unmask(pac::Interrupt::USART1);
}

#[interrupt]
unsafe fn USART2() {
    pac::NVIC::mask(pac::Interrupt::USART2);

    let dw6000 = unsafe { MIDI_DIN_1_PORT.raw_mut() };

    if let Err(err) = dw6000.flush() {
        warn!("Serial flush failed {:?}", err);
    }

    while let Ok(Some(packet)) = dw6000.receive() {
        (MIDI_DIN_1_RX)(PacketList::single(packet));
    }
    pac::NVIC::unmask(pac::Interrupt::USART2);
}

// TODO split output queues (one task per interface)
// #[task(shared = [usb_midi, dw6000, beatstep], capacity = 128, priority = 2)]
fn midi_send(destination: MidiInterface, packets: PacketList) {
    let z = match destination {
        // includes BeatStep
        MidiInterface::USB(_) => unsafe { MIDI_USB_1_PORT.raw_mut() }.transmit(packets),
        MidiInterface::Serial(1) => unsafe { MIDI_DIN_1_PORT.raw_mut() }.transmit(packets),
        MidiInterface::Serial(2) => unsafe { MIDI_DIN_2_PORT.raw_mut() }.transmit(packets),
        dest => {
            Err(MidiError::UnknownInterface(destination))
        }
    };
    if let Err(err) = z {
        info!("Failed to send MIDI: {:?}", destination)
    }
}

// Update the UI - using
// #[task(/*local = [display],*/ capacity = 8)]
async fn midisplay(_text: String) {
    // cx.local.display.print(text).unwrap();
}
