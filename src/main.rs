#![no_main]
#![no_std]
#![feature(slice_as_chunks)]
#![feature(alloc_error_handler)]

#[macro_use]
extern crate rtt_target;

#[macro_use]
extern crate bitfield;

extern crate cortex_m;

extern crate stm32f4xx_hal as hal;

use core::alloc::Layout;
use core::result::Result;
use core::sync::atomic::AtomicU16;

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};
use cortex_m::asm;
use cortex_m::asm::delay;
// STM32 universal (?)
use hal::{
    // renamed for RTIC genericity
    gpio::{
        gpioc::PC13, GpioExt, Output,
        PushPull,
    },
    i2c::I2c,
    otg_fs::{USB, UsbBus, UsbBusType},
    rcc::RccExt,
    serial::{self, config::StopBits, Serial},
    // stm32 as device,
    time::U32Ext,
    stm32,
};
use ssd1306::{Builder, I2CDIBuilder};

use hal::prelude::_embedded_hal_digital_v2_OutputPin;
use panic_rtt_target as _;
use rtic::app;
use usb_device::bus;

use midi::{CableNumber, MidiClass, SerialMidi, usb_device};
use midi::{Interface, Packet, Receive, Transmit};

use crate::apps::dw6000_control::Dw6000Control;
use crate::midi::{channel, print_message, Service, Note, Route, Binding, print_packets};
use alloc::string::String;
use crate::time::Tasks;
use rtic::cyccnt::U32Ext as _;
use crate::apps::blinky_beat::BlinkyBeat;
use crate::midi::Binding::Src;
use alloc::vec::Vec;

use embedded_graphics::image::Image;
use ili9486::gpio::GPIO8ParallelInterface;
// use tinytga::Tga;

use embedded_graphics::{
    fonts::{Font6x8, Text},
    pixelcolor::{Rgb565, Rgb888},
    prelude::*,
    style::{PrimitiveStyle, TextStyle},
};

use ili9486::color::PixelFormat;
use ili9486::io::stm32f4xx::gpioa::*;
use ili9486::io::stm32f4xx::gpiob::*;
use ili9486::io::stm32f4xx::*;

use ili9486::{Command, Commands, ILI9486};

use cortex_m_rt::entry;

use stm32f4xx_hal::{
    gpio::{PullDown},
};


use embedded_graphics::style::PrimitiveStyleBuilder;
use embedded_graphics::primitives::{Rectangle, Circle};
use hal::stm32::Peripherals;
use hal::delay::Delay;
use embedded_hal::blocking::delay::DelayUs;
use crate::apps::bounce::Bounce;
use crate::display::gpio8b::GPIO8BParallelInterface;
use crate::display::gpio8a::GPIO8aParallelInterface;
use crate::display::gpio8a::RawGPIO;
use crate::display::nogpio::NoGPIO;


mod time;
mod midi;
mod devices;
mod apps;
mod display;

pub const CPU_FREQ: u32 = 96_000_000;
pub const CYCLES_PER_MICROSEC: u32 = CPU_FREQ / 1_000_000;
pub const CYCLES_PER_MILLISEC: u32 = CPU_FREQ / 1_000;

pub const AHB_FREQ: u32 = CPU_FREQ / 2;

const LED_BLINK: u32 = CPU_FREQ / 4;
const TASKS_PERIOD: u32 = CYCLES_PER_MILLISEC;

static mut USB_EP_MEMORY: [u32; 1024] = [0; 1024];

#[macro_use]
extern crate alloc;

// define what happens in an Out Of Memory (OOM) condition
#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    asm::bkpt();
    loop {}
}

const FAST_HEAP_SIZE: usize = 16 * 1024;
// 32 KB
const HEAP_SIZE: usize = 48 * 1024;
// 96KB
const LEAF_SIZE: usize = 16;

pub static mut FAST_HEAP: [u8; FAST_HEAP_SIZE] = [0u8; FAST_HEAP_SIZE];
pub static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

#[cfg_attr(not(test), global_allocator)]
static ALLOC: NonThreadsafeAlloc = unsafe {
    let fast_param = FastAllocParam::new(FAST_HEAP.as_ptr(), FAST_HEAP_SIZE);
    let buddy_param = BuddyAllocParam::new(HEAP.as_ptr(), HEAP_SIZE, LEAF_SIZE);
    NonThreadsafeAlloc::new(fast_param, buddy_param)
};

pub type Handle = u16;

pub static NEXT_HANDLE: AtomicU16 = AtomicU16::new(0);


struct CortexDelay;

impl DelayUs<u32> for CortexDelay {
    fn delay_us(&mut self, us: u32) {
        cortex_m::asm::delay(us * CYCLES_PER_MICROSEC)
    }
}

const MODE_INPUT: u32 = 0x00000000;
const MODE_OUTPUT: u32 = 0b_0101_0101_0101_0101_0101_0101_0101_0101;
const TYPE_OUT: u32 = 0x0000FFFF;
const PULL_DOWN_INPUT: u32 = 0b_1010_1010_1010_1010_1010_1010_1010_1010;
const NO_PULL: u32 = 0b_0;
const OUTPUT_SPEED: u32 = 0x0000FFFF;

#[rtic::app(device = hal::pac, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        tasks: Tasks,
        chaos: nanorand::WyRand,
        on_board_led: PC13<Output<PushPull>>,

        // display: display::gui::Display<GPIOBParallelInterface<
        //     PB0IOPin<PullDown, PushPull>, PB1IOPin<PullDown, PushPull>, PB2IOPin<PullDown, PushPull>, PB3IOPin<PullDown, PushPull>,
        //     PB4IOPin<PullDown, PushPull>, PB5IOPin<PullDown, PushPull>, PB6IOPin<PullDown, PushPull>, PB7IOPin<PullDown, PushPull>,
        //     PA9IOPin<PullDown, PushPull>, PA10IOPin<PullDown, PushPull>, PA5IOPin<PullDown, PushPull>, PA6IOPin<PullDown, PushPull>>>,
        // display: display::gui::Display<GPIO8aParallelInterface<
        //     stm32f4::stm32f411::GPIOB,
        //     PA9IOPin<PullDown, PushPull>, PA10IOPin<PullDown, PushPull>, PA5IOPin<PullDown, PushPull>, PA6IOPin<PullDown, PushPull>>>,

        // gpiob: stm32f4::stm32f411::GPIOB,
        gpiob: stm32f4::stm32f411::GPIOB,
        display: display::gui::Display<NoGPIO>,
        midi_router: midi::Router,
        usb_midi: midi::UsbMidi,
        serial_midi: SerialMidi,
    }

    #[init(schedule = [tasks])]
    fn init(cx: init::Context) -> init::LateResources {
        // RTIC needs statics to go first
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        rtt_init_print!();
        rprintln!("Initializing");

        let mut core: rtic::Peripherals = cx.core;
        core.DCB.enable_trace();
        core.DWT.enable_cycle_counter();

        let dev: stm32::Peripherals = cx.device;
        let rcc = dev.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(CPU_FREQ.hz()).freeze();

        let gpioa = dev.GPIOA.split();
        let gpiob = dev.GPIOB.split();
        let gpioc = dev.GPIOC.split();

        let mut tasks = time::Tasks::default();
        cx.schedule.tasks(cx.start).unwrap();

        let on_board_led = gpioc.pc13.into_push_pull_output();

        // Setup Display
        // let scl = gpiob.pb8.into_alternate_af4().set_open_drain();
        // let sda = gpiob.pb9.into_alternate_af4().set_open_drain();
        //
        // let i2c = I2c::i2c1(dev.I2C1, (scl, sda), 400.khz(), clocks);
        // let interface = I2CDIBuilder::new().init(i2c);
        // let mut oled: GraphicsMode<_> = Builder::new().connect(interface).into();
        // oled.init().unwrap();
        //
        // display::draw_logo(&mut oled).unwrap();

        // let d0 = GPIOB::PB0::<PullDown, PushPull>(gpiob.pb0.into_pull_down_input());
        // let d1 = GPIOB::PB1::<PullDown, PushPull>(gpiob.pb1.into_pull_down_input());
        // let d2 = GPIOB::PB2::<PullDown, PushPull>(gpiob.pb2.into_pull_down_input());
        // let d3 = GPIOB::PB3::<PullDown, PushPull>(gpiob.pb3.into_pull_down_input());
        //
        // let d4 = GPIOB::PB4::<PullDown, PushPull>(gpiob.pb4.into_pull_down_input());
        // let d5 = GPIOB::PB5::<PullDown, PushPull>(gpiob.pb5.into_pull_down_input());
        // let d6 = GPIOB::PB6::<PullDown, PushPull>(gpiob.pb6.into_pull_down_input());
        // let d7 = GPIOB::PB7::<PullDown, PushPull>(gpiob.pb7.into_pull_down_input());

        let cs = GPIOA::PA9::<PullDown, PushPull>(gpioa.pa9.into_pull_down_input());
        let dc = GPIOA::PA10::<PullDown, PushPull>(gpioa.pa10.into_pull_down_input());
        let wr = GPIOA::PA6::<PullDown, PushPull>(gpioa.pa6.into_pull_down_input());
        let rd = GPIOA::PA5::<PullDown, PushPull>(gpioa.pa5.into_pull_down_input());

        // let parallel_gpio = GPIO8BParallelInterface::new(d0, d1, d2, d3, d4, d5, d6, d7, cs, dc, rd, wr).unwrap();
        // let parallel_gpio = GPIO8aParallelInterface::new(dev.GPIOB, cs, dc, rd, wr).unwrap();
        let parallel_gpio = NoGPIO {  };

        let rst = GPIOA::PA8::<PullDown, PushPull>(gpioa.pa8.into_pull_down_input());
        let mut lcd = ILI9486::new(&mut CortexDelay {}, PixelFormat::Rgb565, parallel_gpio, rst).unwrap();

        rprintln!("Screen OK");

        let tx_pin = gpioa.pa2.into_alternate_af7();
        let rx_pin = gpioa.pa3.into_alternate_af7();
        let mut uart = Serial::usart2(
            dev.USART2,
            (tx_pin, rx_pin),
            serial::config::Config::default()
                .baudrate(31250.bps())
                .stopbits(StopBits::STOP1)
                .parity_none(),
            clocks,
        ).unwrap();
        uart.listen(serial::Event::Rxne);
        let serial_midi = SerialMidi::new(uart, CableNumber::MIN);
        rprintln!("Serial port OK");

        let usb = USB {
            usb_global: dev.OTG_FS_GLOBAL,
            usb_device: dev.OTG_FS_DEVICE,
            usb_pwrclk: dev.OTG_FS_PWRCLK,
            pin_dm: gpioa.pa11.into_alternate_af10(),
            pin_dp: gpioa.pa12.into_alternate_af10(),
            hclk: AHB_FREQ.hz(),
        };
        *USB_BUS = Some(UsbBus::new(usb, unsafe { &mut USB_EP_MEMORY }));
        let usb_bus = USB_BUS.as_ref().unwrap();
        let midi_class = MidiClass::new(usb_bus);
        // USB devices init _after_ classes
        let usb_dev = usb_device(usb_bus);

        let chaos = nanorand::WyRand::new_seed(0);

        let mut midi_router: midi::Router = midi::Router::default();

        // let _usb_echo = midi_router.add_route(
        //     Route::echo(Interface::USB(0))
        //         .filter(|_now, cx| print_message(cx)));

        // let _serial_print = midi_router.bind(Route::from(Interface::Serial(0)).filter(print_message()));

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

        let mut dwctrl = Dw6000Control::new((Interface::Serial(0), channel(1)), (Interface::USB(0), channel(1)));
        dwctrl.start(cx.start, &mut midi_router, &mut tasks).unwrap();

        let mut bbeat = BlinkyBeat::new((Interface::USB(0), channel(1)), vec![Note::C1m, Note::Cs1m, Note::B1m, Note::G0]);
        bbeat.start(cx.start, &mut midi_router, &mut tasks).unwrap();

        let mut bounce = Bounce::new();
        bounce.start(cx.start, &mut midi_router, &mut tasks).unwrap();

        rprintln!("Routes OK");

        rprintln!("-> Initialized");

        init::LateResources {
            tasks,
            chaos,
            on_board_led,
            display: display::gui::Display::new(lcd).unwrap(),
            gpiob: dev.GPIOB,
            midi_router,
            serial_midi,
            usb_midi: midi::UsbMidi {
                dev: usb_dev,
                midi_class,
            },
        }
    }

    #[idle(resources = [on_board_led, gpiob])]
    fn idle(cx: idle::Context) -> ! {
        let mut led_on = false;

        let mut gpiob: &mut stm32f4::stm32f411::GPIOB = cx.resources.gpiob;
        let pins = gpiob.split();
        let mut pb3 = pins.pb3.into_open_drain_output();

        // gpiob.pupdr.modify(|r, w| unsafe {
        //     w.bits(r.bits() | NO_PULL)
        // });
        // gpiob.moder.modify(|r, w| unsafe {
        //     w.bits(r.bits() | MODE_OUTPUT)
        // });

        loop {
            if led_on {
                // gpiob.write_byte(0xFFFFFFFF);
                pb3.set_high();
                cx.resources.on_board_led.set_high().unwrap();
            } else {
                // gpiob.write_byte(0);
                pb3.set_low();
                cx.resources.on_board_led.set_low().unwrap();
            }
            led_on = !led_on;
            delay(LED_BLINK);
        }
    }

    /// USB polling required every 0.125 millisecond
    #[task(binds = OTG_FS_WKUP, resources = [usb_midi], priority = 3)]
    fn usb_poll(cx: usb_poll::Context) {
        let _ = cx.resources.usb_midi.poll();
    }

    /// USB receive interrupt
    #[task(binds = OTG_FS, spawn = [midispatch], resources = [usb_midi], priority = 3)]
    fn usb_interrupt(cx: usb_interrupt::Context) {
        // poll() is also required here else receive may block forever
        if cx.resources.usb_midi.poll() {
            while let Some(packet) = cx.resources.usb_midi.receive().unwrap() {
                if let Err(e) = cx.spawn.midispatch(Src(Interface::USB(0)), vec![packet]) {
                    rprintln!("Dropped incoming MIDI: {:?}", e)
                }
            }
        }
    }

    /// Serial receive interrupt
    #[task(binds = USART2, spawn = [midispatch], resources = [serial_midi], priority = 3)]
    fn serial_irq0(cx: serial_irq0::Context) {
        if let Err(err) = cx.resources.serial_midi.flush() {
            rprintln!("Serial flush failed {:?}", err);
        }

        while let Ok(Some(packet)) = cx.resources.serial_midi.receive() {
            cx.spawn.midispatch(Src(Interface::Serial(0)), vec![packet]).unwrap();
        }
    }

    #[task(resources = [chaos, tasks], spawn = [midispatch, midisplay], schedule = [tasks], priority = 3)]
    fn tasks(mut cx: tasks::Context) {
        let tasks = &mut cx.resources.tasks;
        let chaos = &mut cx.resources.chaos;
        let spawn = &mut cx.spawn;

        tasks.handle(cx.scheduled, chaos, spawn);

        cx.schedule.tasks(cx.scheduled + TASKS_PERIOD.cycles()).unwrap();
    }

    #[task(spawn = [midisend, midisplay], resources = [midi_router, tasks], priority = 3, capacity = 16)]
    fn midispatch(cx: midispatch::Context, binding: Binding, packets: Vec<Packet>) {
        let router: &mut midi::Router = cx.resources.midi_router;
        router.midispatch(cx.scheduled, packets, binding, cx.spawn).unwrap();
    }

    #[task(resources = [usb_midi, serial_midi], capacity = 128, priority = 2)]
    fn midisend(mut cx: midisend::Context, interface: Interface, packets: Vec<Packet>) {
        match interface {
            Interface::USB(_) => {
                cx.resources.usb_midi.lock(
                    |usb_midi| if let Err(e) = usb_midi.transmit(packets) {
                        rprintln!("Failed to send USB MIDI: {:?}", e)
                    }
                );
            }
            Interface::Serial(_) => {
                // TODO use proper serial port #
                cx.resources.serial_midi.lock(
                    |serial_out| if let Err(e) = serial_out.transmit(packets) {
                        rprintln!("Failed to send Serial MIDI: {:?}", e)
                    });
            }
            Interface::Application(_) => {}
        }
    }

    #[task(resources = [display], capacity = 8)]
    fn midisplay(ctx: midisplay::Context, text: String) {
        ctx.resources.display.print(text).unwrap()
    }

    extern "C" {
        // Reuse some interrupts for software task scheduling.
        fn EXTI0();
        fn EXTI1();
        fn USART1();
    }
};
