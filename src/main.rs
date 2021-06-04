#![no_main]
#![no_std]
#![feature(slice_as_chunks)]
#![feature(alloc_error_handler)]

#[macro_use]
extern crate rtt_target;

#[macro_use]
extern crate bitfield;

extern crate cortex_m;

#[macro_use]
extern crate display_interface_parallel_gpio;

extern crate stm32f4xx_hal as hal;

use core::alloc::Layout;
use core::result::Result;
use core::sync::atomic::AtomicU16;


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
// use ssd1306::{Builder, I2CDIBuilder};

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


use embedded_graphics::{
    fonts::{Font6x8, Text},
    pixelcolor::{Rgb565, Rgb888},
    prelude::*,
    style::{PrimitiveStyle, TextStyle},
};

use cortex_m_rt::entry;

use stm32f4xx_hal::{
    gpio::{PullDown},
};


use hal::stm32::Peripherals;
use hal::delay::Delay;
use embedded_hal::blocking::delay::{DelayUs, DelayMs};
use crate::apps::bounce::Bounce;

use hal::gpio::gpiob::*;
use hal::gpio::{Input, Alternate};

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};
use hal::gpio::gpioa::*;
use display_interface_parallel_gpio::{PGPIO8BitInterface, Generic8BitBus};
use ili9486::{ILI9486, DisplayError, Orientation, DisplaySize320x480, DisplayMode};
use crate::display::gui;

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

const DW6000: Interface = Interface::Serial(0);
const BEATSTEP: Interface = Interface::Serial(1);

#[rtic::app(device = hal::pac, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        tasks: Tasks,
        chaos: nanorand::WyRand,
        on_board_led: PC13<Output<PushPull>>,
        // PB0IOPin<PullDown, PushPull>, PB1IOPin<PullDown, PushPull>, PB2IOPin<PullDown, PushPull>, PB3IOPin<PullDown, PushPull>,
        // PB4IOPin<PullDown, PushPull>, PB12IOPin<PullDown, PushPull>, PB13IOPin<PullDown, PushPull>, PB14IOPin<PullDown, PushPull>
        display: gui::Display<ILI9486<PGPIO8BitInterface<Generic8BitBus<PB0<Output<PushPull>>, PB1<Output<PushPull>>, PB2<Output<PushPull>>,
            PB3<Output<PushPull>>, PB4<Output<PushPull>>, PB12<Output<PushPull>>, PB13<Output<PushPull>>, PB14<Output<PushPull>>>,
            PA10<Output<PushPull>>, PA6<Output<PushPull>>>, PA8<Output<PushPull>>>, Rgb565>,
        // display: display::gui::Display<GPIO8aParallelInterface<
        //     stm32f4::stm32f411::GPIOB,
        //     PA9IOPin<PullDown, PushPull>, PA10IOPin<PullDown, PushPull>, PA5IOPin<PullDown, PushPull>, PA6IOPin<PullDown, PushPull>>>,

        // gpiob: stm32f4::stm32f411::GPIOB,

        // pb3: PB3<Output<PushPull>>,
        // display: display::gui::Display<NoGPIO>,
        midi_router: midi::Router,
        usb_midi: midi::UsbMidi,
        beatstep: SerialMidi<hal::stm32::USART1, (PB6<Alternate<hal::gpio::AF7>>, PB7<Alternate<hal::gpio::AF7>>)>,
        dw6000: SerialMidi<hal::stm32::USART2, (PA2<Alternate<hal::gpio::AF7>>, PA3<Alternate<hal::gpio::AF7>>)>,
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

        unsafe { dev.GPIOB.ospeedr.modify(|_, w| w.bits(0xFFFFFFFF)); }
        unsafe { dev.GPIOA.ospeedr.modify(|_, w| w.bits(0xFFFFFFFF)); }

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

        let d0 = gpiob.pb0.into_push_pull_output();
        let d1 = gpiob.pb1.into_push_pull_output();
        let d2 = gpiob.pb2.into_push_pull_output();
        let d3 = gpiob.pb3.into_push_pull_output();

        let d4 = gpiob.pb4.into_push_pull_output();
        let d5 = gpiob.pb12.into_push_pull_output();
        let d6 = gpiob.pb13.into_push_pull_output();
        let d7 = gpiob.pb14.into_push_pull_output();

        // let cs = gpioa.pa9.into_pull_down_input());
        let dc = gpioa.pa10.into_push_pull_output();
        let wr = gpioa.pa6.into_push_pull_output();
        // let rd = gpioa.pa5.into_pull_down_input());
        let bus = Generic8BitBus::new((d0, d1, d2, d3, d4, d5, d6, d7)).unwrap();

        let parallel_gpio = PGPIO8BitInterface::new(bus, dc, wr);

        let rst = gpioa.pa8.into_push_pull_output();
        let mut lcd = ILI9486::new(parallel_gpio, rst, &mut CortexDelay{}, DisplayMode::default(), DisplaySize320x480).unwrap();

        rprintln!("Screen OK");

        let bs_tx = gpiob.pb6.into_alternate_af7();
        let bs_rx = gpiob.pb7.into_alternate_af7();
        let mut uart1 = Serial::usart1(
            dev.USART1,
            (bs_tx, bs_rx),
            serial::config::Config::default()
                .baudrate(921_600.bps()),
            clocks,
        ).unwrap();
        uart1.listen(serial::Event::Rxne);
        let beatstep = SerialMidi::new(uart1, CableNumber::MIN);
        rprintln!("BeatStep MIDI ports OK");

        let dw_tx = gpioa.pa2.into_alternate_af7();
        let dw_rx = gpioa.pa3.into_alternate_af7();
        let mut uart2 = Serial::usart2(
            dev.USART2,
            (dw_tx, dw_rx),
            serial::config::Config::default()
                .baudrate(31250.bps()),
            clocks,
        ).unwrap();
        uart2.listen(serial::Event::Rxne);
        let dw6000 = SerialMidi::new(uart2, CableNumber::MIN);
        rprintln!("DW6000 MIDI ports OK");

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

        let mut dwctrl = Dw6000Control::new((DW6000, channel(1)), (BEATSTEP, channel(1)));
        dwctrl.start(cx.start, &mut midi_router, &mut tasks).unwrap();

        let mut bbeat = BlinkyBeat::new((BEATSTEP, channel(1)), vec![Note::C1m, Note::Cs1m, Note::B1m, Note::G0]);
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
            midi_router,
            beatstep,
            dw6000,
            usb_midi: midi::UsbMidi {
                dev: usb_dev,
                midi_class,
            },
        }
    }

    #[idle(resources = [on_board_led])]
    fn idle(cx: idle::Context) -> ! {
        let mut led_on = false;

        loop {
            if led_on {
                cx.resources.on_board_led.set_high().unwrap();
            } else {
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
    /// Using LOWER priority to backoff on USB reception if Serial queues not emptying fast enough
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
    #[task(binds = USART1, spawn = [midispatch], resources = [beatstep], priority = 3)]
    fn usart1_irq(cx: usart1_irq::Context) {
        if let Err(err) = cx.resources.beatstep.flush() {
            rprintln!("Serial flush failed {:?}", err);
        }

        while let Ok(Some(packet)) = cx.resources.beatstep.receive() {
            cx.spawn.midispatch(Src(BEATSTEP), vec![packet]).unwrap();
        }
    }

    /// Serial receive interrupt
    #[task(binds = USART2, spawn = [midispatch], resources = [dw6000], priority = 3)]
    fn usart2_irq(cx: usart2_irq::Context) {
        if let Err(err) = cx.resources.dw6000.flush() {
            rprintln!("Serial flush failed {:?}", err);
        }

        while let Ok(Some(packet)) = cx.resources.dw6000.receive() {
            cx.spawn.midispatch(Src(DW6000), vec![packet]).unwrap();
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

    // TODO split output queues (one task per interface)
    #[task(resources = [usb_midi, dw6000, beatstep], capacity = 128, priority = 2)]
    fn midisend(mut cx: midisend::Context, interface: Interface, packets: Vec<Packet>) {
        match interface {
            Interface::USB(_) => cx.resources.usb_midi.lock(
                |midi| if let Err(e) = midi.transmit(packets) {
                    rprintln!("Failed to send USB MIDI: {:?}", e)
                }),

            DW6000 => cx.resources.dw6000.lock(
                |midi| if let Err(e) = midi.transmit(packets) {
                    rprintln!("Failed to send Serial MIDI: {:?}", e)
                }),

            BEATSTEP => cx.resources.beatstep.lock(
                |midi| if let Err(e) = midi.transmit(packets) {
                    rprintln!("Failed to send Serial MIDI: {:?}", e)
                }),
            _ => {}
        }
    }

    // Update the UI - using
    #[task(resources = [display], capacity = 8)]
    fn midisplay(ctx: midisplay::Context, text: String) {
        ctx.resources.display.print(text).unwrap()
    }

    extern "C" {
        // Reuse some interrupts for software task scheduling.
        fn EXTI0();
        fn EXTI1();
        fn USART6();
    }
};


