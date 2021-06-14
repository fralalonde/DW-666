#![no_main]
#![no_std]
#![feature(slice_as_chunks)]

#![feature(alloc_error_handler)]

#[macro_use]
extern crate rtt_target;

#[macro_use]
extern crate bitfield;

extern crate cortex_m;
extern crate minimidi as midi;

use alloc::string::String;
use core::alloc::Layout;
use core::result::Result;
use core::sync::atomic::AtomicU16;

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};
use cortex_m::asm;
use cortex_m_rt::entry;
use display_interface_spi::SPIInterface;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_hal::{
    blocking::delay::{DelayMs, DelayUs},
    digital::v2::OutputPin,
    spi as espi,
};
use hal::{
    delay::Delay,
    gpio::{
        {Alternate, Input},
        gpioa::*,
        gpiob::*,
        gpioc::PC13, GpioExt, Output,
        PushPull,
    },
    i2c::I2c,
    otg_fs::{USB, UsbBus, UsbBusType},
    rcc::RccExt,
    serial::{self, Serial},
    spi,
    stm32,
    time::U32Ext,
};
use hal::spi::{Mode, NoMiso, Phase, Polarity, Spi};
use heapless::Vec;
use ili9341::Ili9341;
use panic_rtt_target as _;
use rtic::cyccnt::U32Ext as _;
use usb_device::bus;

use midi::{CableNumber};
use midi::{Interface, Packet, Receive, Transmit};

use crate::apps::blinky_beat::BlinkyBeat;
use crate::apps::bounce::Bounce;
use crate::apps::dw6000_control::Dw6000Control;
// use display_interface_parallel_gpio::{PGPIO8BitInterface, Generic8BitBus};
// use ili9486::{ILI9486, DisplaySize320x480, DisplayMode};
use crate::display::gui;
use crate::display::gui::Display;
use midi::{Binding, channel, Note, PacketList};
use crate::Binding::Src;
use crate::time::Tasks;
use crate::route::Service;

extern crate stm32f4xx_hal as hal;

// use ssd1306::{Builder, I2CDIBuilder};

mod time;
mod devices;
mod apps;
mod display;
mod route;
mod filter;
mod sysex;
mod port;

pub const CPU_FREQ: u32 = 48_000_000;
// pub const APB1_FREQ: u32 = CPU_FREQ / 2;
// pub const APB2_FREQ: u32 = CPU_FREQ;

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


pub struct CortexDelay;

impl DelayUs<u16> for CortexDelay {
    fn delay_us(&mut self, us: u16) {
        asm::delay(us as u32 * CYCLES_PER_MICROSEC)
    }
}

impl DelayMs<u16> for CortexDelay {
    fn delay_ms(&mut self, us: u16) {
        asm::delay(us as u32 * CYCLES_PER_MILLISEC)
    }
}

const DW6000: Interface = Interface::Serial(0);
const BEATSTEP: Interface = Interface::USB(0);

#[rtic::app(device = hal::pac, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        tasks: Tasks,
        chaos: nanorand::WyRand,
        on_board_led: PC13<Output<PushPull>>,

        display: gui::Display<Ili9341<SPIInterface<Spi<hal::stm32::SPI1, (PA5<Alternate<hal::gpio::AF5>>, NoMiso, PA7<Alternate<hal::gpio::AF5>>)>, PB0<Output<PushPull>>, PA4<Output<PushPull>>>, PA6<Output<PushPull>>>, Rgb565>,

        midi_router: route::Router,
        usb_midi: port::usb::UsbMidi,
        // beatstep: SerialMidi<hal::stm32::USART1, (PB6<Alternate<hal::gpio::AF7>>, PB7<Alternate<hal::gpio::AF7>>)>,
        dw6000: port::serial::SerialMidi<hal::stm32::USART2, (PA2<Alternate<hal::gpio::AF7>>, PA3<Alternate<hal::gpio::AF7>>)>,
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
        let clocks = rcc.cfgr
            .sysclk(CPU_FREQ.hz())
            // .pclk1(APB1_FREQ.hz())
            // .pclk2(APB2_FREQ.hz())
            .freeze();

        // unsafe { dev.GPIOB.ospeedr.modify(|_, w| w.bits(0xFFFFFFFF)); }
        // unsafe { dev.GPIOA.ospeedr.modify(|_, w| w.bits(0xFFFFFFFF)); }

        let gpioa = dev.GPIOA.split();
        let gpiob = dev.GPIOB.split();
        let gpioc = dev.GPIOC.split();

        let mut tasks = time::Tasks::default();
        cx.schedule.tasks(cx.start).unwrap();

        let on_board_led = gpioc.pc13.into_push_pull_output();

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

        let mut delay = CortexDelay {};

        let sclk = gpioa.pa5.into_alternate_af5();
        // let miso = gpioa.pa6.into_alternate_af5();
        let mosi = gpioa.pa7.into_alternate_af5();

        let spi = spi::Spi::spi1(
            dev.SPI1,
            (sclk, NoMiso, mosi),
            espi::MODE_0,
            100.khz().into(),
            clocks,
        );

        let lcd_cs = gpioa.pa4.into_push_pull_output();
        let lcd_dc = gpiob.pb0.into_push_pull_output();

        let lcd_spi = SPIInterface::new(spi, lcd_dc, lcd_cs);

        let mut ts_cs = gpiob.pb1.into_push_pull_output();
        ts_cs.set_high().expect("Could not disable touchscreen");

        let lcd_reset = gpioa.pa6.into_push_pull_output();
        let ili9341 = Ili9341::new(lcd_spi, lcd_reset, &mut delay).expect("LCD init failed");

        let mut display = Display::new(ili9341).unwrap();

        rprintln!("Screen OK");

        // let bs_tx = gpiob.pb6.into_alternate_af7();
        // let bs_rx = gpiob.pb7.into_alternate_af7();
        // let mut uart1 = Serial::usart1(
        //     dev.USART1,
        //     (bs_tx, bs_rx),
        //     serial::config::Config::default()
        //         .baudrate(921_600.bps()),
        //     clocks,
        // ).unwrap();
        // uart1.listen(serial::Event::Rxne);
        // let beatstep = SerialMidi::new(uart1, CableNumber::MIN);
        // rprintln!("BeatStep MIDI port OK");

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
        let dw6000 = port::serial::SerialMidi::new(uart2, CableNumber::MIN);
        rprintln!("DW6000 MIDI port OK");

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
        let midi_class = port::usb::MidiClass::new(usb_bus);
        // USB devices init _after_ classes
        let usb_dev = usb_device(usb_bus);
        rprintln!("USB dev OK");

        let chaos = nanorand::WyRand::new_seed(0);
        rprintln!("Chaos OK");

        let mut midi_router: route::Router = route::Router::default();
        rprintln!("Router OK");

        // let _usb_echo = midi_router.add_route(
        //     Route::echo(Interface::USB(0))
        //         .filter(|_now, cx| print_message(cx)));

        let _serial_print = midi_router
            .add_route(route::Route::from(DW6000)
                .filter(|t, cx| filter::print_message(cx)));

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

        // let mut bounce = Bounce::new();
        // bounce.start(cx.start, &mut midi_router, &mut tasks).unwrap();

        rprintln!("-> Initialized");

        init::LateResources {
            tasks,
            chaos,
            on_board_led,
            display,
            midi_router,
            // beatstep,
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
            asm::delay(LED_BLINK);
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
                if let Err(e) = cx.spawn.midispatch(Src(BEATSTEP), PacketList::single(packet)) {
                    rprintln!("Dropped incoming MIDI: {:?}", e)
                }
            }
        }
    }

    // /// Serial receive interrupt
    // #[task(binds = USART1, spawn = [midispatch], resources = [beatstep], priority = 3)]
    // fn usart1_irq(cx: usart1_irq::Context) {
    //     if let Err(err) = cx.resources.beatstep.flush() {
    //         rprintln!("Serial flush failed {:?}", err);
    //     }
    //
    //     loop {
    //         match cx.resources.beatstep.receive() {
    //             Ok(Some(packet)) => {
    //                 rprintln!("MIDI from beatstep {:?}", packet);
    //                 cx.spawn.midispatch(Src(BEATSTEP), PacketList::single(packet)).unwrap();
    //                 continue;
    //             }
    //             Err(e) => {rprintln!("Error serial read {:?}", e); break},
    //             _ => {break;}
    //         }
    //     }
    // }

    /// Serial receive interrupt
    #[task(binds = USART2, spawn = [midispatch], resources = [dw6000], priority = 3)]
    fn usart2_irq(cx: usart2_irq::Context) {
        if let Err(err) = cx.resources.dw6000.flush() {
            rprintln!("Serial flush failed {:?}", err);
        }

        while let Ok(Some(packet)) = cx.resources.dw6000.receive() {
            cx.spawn.midispatch(Src(DW6000), PacketList::single(packet)).unwrap();
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
    fn midispatch(cx: midispatch::Context, binding: Binding, packets: PacketList) {
        let router: &mut route::Router = cx.resources.midi_router;
        router.midispatch(cx.scheduled, packets, binding, cx.spawn).unwrap();
    }

    // TODO split output queues (one task per interface)
    #[task(resources = [usb_midi, dw6000, /*beatstep*/], capacity = 128, priority = 2)]
    fn midisend(mut cx: midisend::Context, interface: Interface, packets: PacketList) {
        match interface {
            Interface::USB(_) => cx.resources.usb_midi.lock(
                |midi| if let Err(e) = midi.transmit(packets) {
                    rprintln!("Failed to send USB MIDI: {:?}", e)
                }),

            DW6000 => cx.resources.dw6000.lock(
                |midi| if let Err(e) = midi.transmit(packets) {
                    rprintln!("Failed to send Serial MIDI: {:?}", e)
                }),

            // BEATSTEP => cx.resources.beatstep.lock(
            //     |midi| if let Err(e) = midi.transmit(packets) {
            //         rprintln!("Failed to send Serial MIDI: {:?}", e)
            //     }),
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


