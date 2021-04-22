#![no_main]
#![no_std]
#![feature(slice_as_chunks)]
#![feature(alloc_error_handler)]
#![feature(new_uninit)]

// #[macro_use]
// extern crate enum_map;

#[macro_use]
extern crate rtt_target;

#[macro_use]
extern crate bitfield;

extern crate cortex_m;

extern crate stm32f4xx_hal as hal;

mod event;
// mod rtc;
mod clock;
mod input;
mod midi;
mod output;
mod app;
mod devices;
mod dw6000_control;

use embedded_hal::digital::v2::OutputPin;
use rtic::app;
use rtic::cyccnt::U32Ext as _;

use ssd1306::{prelude::*, Builder, I2CDIBuilder};

use usb_device::bus;

use input::{Scan, Controls};

use midi::{SerialMidi, MidiClass, CableNumber, usb_device, Route};
use midi::{Packet, Interface, Transmit, Receive};
use core::result::Result;

use panic_rtt_target as _;
// use crate::app::AppState;
// use crate::clock::{CPU_FREQ, PCLK1_FREQ};

// STM32 universal (?)
use hal::{
    // renamed for RTIC genericity
    stm32 as device,
    serial::{self, Serial, Rx, Tx, config::StopBits},
    stm32::USART2,
    stm32::Peripherals,
    gpio::{
        GpioExt,
        AF4,
        Input, PullUp, Output, PushPull,
        gpioa::{PA6, PA7},
        gpioc::{PC13},
    },
    otg_fs::{UsbBusType, UsbBus, USB},
    rcc::RccExt,
    time::U32Ext,
    timer::Timer,
    i2c::I2c,
};
use crate::devices::dsi_evolver;

pub const CPU_FREQ: u32 = 96_000_000;
pub const MICRO: u32 = CPU_FREQ / 1_000_000;
pub const MILLI: u32 = CPU_FREQ / 1_000;

const CTL_SCAN: u32 = 500 * MICRO;
const LED_BLINK: u32 = CPU_FREQ / 4;

const ARP_NOTE_LEN: u32 = 7200000;

static mut USB_EP_MEMORY: [u32; 1024] = [0; 1024];

#[macro_use]
extern crate alloc;

use core::alloc::Layout;
use cortex_m::asm;

// define what happens in an Out Of Memory (OOM) condition
#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    asm::bkpt();
    loop {}
}

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};
use crate::midi::{capture_sysex, print_tag, event_print, Channel, Cull, Service};
use crate::dw6000_control::Dw6000Control;
use cortex_m::asm::delay;

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

#[app(device = crate::device, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        // clock: rtc::RtcClock,
        on_board_led: PC13<Output<PushPull>>,
        controls: input::Controls<PA6<Input<PullUp>>, PA7<Input<PullUp>>>,
        app_state: app::AppState,
        display: output::Display,
        midi_router: midi::Router,
        usb_midi: midi::UsbMidi,
        serial_midi: SerialMidi,
    }

    #[init(schedule = [control_scan])]
    fn init(cx: init::Context) -> init::LateResources {
        // RTIC needs statics to go first
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        rtt_init_print!();
        rprintln!("Initializing");

        let peripherals = cx.device;
        let rcc = peripherals.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(CPU_FREQ.hz()).freeze();

        let gpioa = peripherals.GPIOA.split();
        let gpiob = peripherals.GPIOB.split();
        let gpioc = peripherals.GPIOC.split();

        let on_board_led = gpioc.pc13.into_push_pull_output();

        let encoder = input::encoder(
            event::RotaryId::MAIN,
            gpioa.pa6.into_pull_up_input(),
            gpioa.pa7.into_pull_up_input(),
        );
        // let _enc_push = gpioa.pa5.into_pull_down_input(&mut gpioa.crl);
        let controls = Controls::new(encoder);
        cx.schedule.control_scan(cx.start + CTL_SCAN.cycles()).unwrap();
        rprintln!("Controls OK");

        // Display
        let scl = gpiob.pb8.into_alternate_af4().set_open_drain();
        let sda = gpiob.pb9.into_alternate_af4().set_open_drain();

        let i2c = I2c::i2c1(peripherals.I2C1, (scl, sda), 400.khz(), clocks);
        let interface = I2CDIBuilder::new().init(i2c);
        let mut oled: GraphicsMode<_> = Builder::new().connect(interface).into();
        oled.init().unwrap();
        output::draw_logo(&mut oled);
        rprintln!("Screen OK");

        let tx_pin = gpioa.pa2.into_alternate_af7();
        let rx_pin = gpioa.pa3.into_alternate_af7();
        let mut uart = Serial::usart2(
            peripherals.USART2,
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
            usb_global: peripherals.OTG_FS_GLOBAL,
            usb_device: peripherals.OTG_FS_DEVICE,
            usb_pwrclk: peripherals.OTG_FS_PWRCLK,
            pin_dm: gpioa.pa11.into_alternate_af10(),
            pin_dp: gpioa.pa12.into_alternate_af10(),
        };
        *USB_BUS = Some(UsbBus::new(usb, unsafe { &mut USB_EP_MEMORY }));
        let usb_bus = USB_BUS.as_ref().unwrap();
        let midi_class = MidiClass::new(usb_bus);
        // USB devices init _after_ classes
        let usb_dev = usb_device(usb_bus);

        let mut midi_router: midi::Router = midi::Router::default();
        // let _usb_echo = midi_router.bind(Route::echo(Interface::USB).filter(event_print()));
        let _serial_print = midi_router.bind(Route::from(Interface::Serial(0)).filter(event_print()));
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

        let mut dwctrl = Dw6000Control::new((Interface::Serial(0), Channel::cull(1)), (Interface::USB, Channel::cull(1)));
        dwctrl.start(&mut midi_router);

        rprintln!("Routes OK");

        rprintln!("-> Initialized");

        init::LateResources {
            // clock,
            controls,
            on_board_led,
            app_state: app::AppState::default(),
            midi_router,
            display: output::Display {
                oled,
            },
            serial_midi,
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
            // rprintln!("_m'bored_");
            delay(LED_BLINK);
        }
    }

    /// USB polling required every 0.125 millisecond
    #[task(binds = OTG_FS_WKUP, resources = [usb_midi], priority = 3)]
    fn usb_poll(cx: usb_poll::Context) {
        let _ = cx.resources.usb_midi.poll();
    }

    /// USB receive interrupt
    #[task(binds = OTG_FS, spawn = [dispatch_from], resources = [usb_midi], priority = 3)]
    fn usb_interrupt(cx: usb_interrupt::Context) {
        // poll() is also required here else receive may block forever
        if cx.resources.usb_midi.poll() {
            while let Some(packet) = cx.resources.usb_midi.receive().unwrap() {
                cx.spawn.dispatch_from(Interface::USB, packet);
            }
        }
    }

    /// Serial receive interrupt
    #[task(binds = USART2, spawn = [dispatch_from], resources = [serial_midi], priority = 3)]
    fn serial_irq0(cx: serial_irq0::Context) {
        if let Err(err) = cx.resources.serial_midi.flush() {
            rprintln!("Serial flush failed {:?}", err);
        }

        while let Ok(Some(packet)) = cx.resources.serial_midi.receive() {
            cx.spawn.dispatch_from(Interface::Serial(0), packet);
        }
    }

    /// Encoder scan timer interrupt
    #[task(resources = [controls], spawn = [dispatch_ctl], schedule = [control_scan], priority = 1)]
    fn control_scan(cx: control_scan::Context) {
        let controls = cx.resources.controls;
        if let Some(event) = controls.scan(clock::long_now()) {
            cx.spawn.dispatch_ctl(event).unwrap();
        }
        cx.schedule.control_scan(cx.scheduled + CTL_SCAN.cycles()).unwrap();
    }

    #[task(spawn = [dispatch_ctl, dispatch_app], resources = [controls, app_state], capacity = 5, priority = 1)]
    fn dispatch_ctl(cx: dispatch_ctl::Context, event: event::CtlEvent) {
        if let Some(derived) = cx.resources.controls.derive(event) {
            cx.spawn.dispatch_ctl(derived);
        }
        if let Some(app_change) = cx.resources.app_state.dispatch_ctl(event) {
            cx.spawn.dispatch_app(app_change);
        }
    }

    #[task(resources = [display], capacity = 5, priority = 1)]
    fn dispatch_app(cx: dispatch_app::Context, event: event::AppEvent) {
        // TODO filter conditional output spawn
        cx.resources.display.update(event)
    }

    // #[task(resources = [app_state], spawn = [dispatch_midi], schedule = [arp_note_off, arp_note_on])]
    // fn arp_note_on(cx: arp_note_on::Context) {
    //     let app_state: &mut AppState = cx.resources.app_state;
    //
    //     let channel = app_state.arp.channel;
    //     let note = app_state.arp.note;
    //     // let velo = Velocity::try_from().unwrap();
    //     app_state.arp.bump();
    //
    //     let note_on = midi::note_on(app_state.arp.channel, app_state.arp.note, 0x7F).unwrap();
    //     cx.spawn.dispatch_midi(Dst(Interface::Serial(0)), note_on.into()).unwrap();
    //
    //     cx.schedule.arp_note_off(cx.scheduled + ARP_NOTE_LEN.cycles(), channel, note).unwrap();
    //     cx.schedule.arp_note_on(cx.scheduled + ARP_NOTE_LEN.cycles()).unwrap();
    // }
    //
    // #[task(spawn = [dispatch_midi], capacity = 2)]
    // fn arp_note_off(cx: arp_note_off::Context, channel: Channel, note: Note) {
    //     let note_off = midi::Message::NoteOff(channel, note, Velocity::try_from(0).unwrap());
    //     cx.spawn.dispatch_midi(Dst(Interface::Serial(0)), note_off.into()).unwrap();
    // }

    #[task(spawn = [send_midi], schedule = [send_midi], resources = [midi_router], priority = 3)]
    fn dispatch_from(cx: dispatch_from::Context, from: Interface, packet: Packet) {
        let router: &mut midi::Router = cx.resources.midi_router;
        router.dispatch_from(cx.scheduled, packet, from, cx.spawn, cx.schedule)
    }

    #[task(resources = [usb_midi, serial_midi], capacity = 64, priority = 2)]
    fn send_midi(mut cx: send_midi::Context, interface: Interface, packet: Packet) {
        match interface {
            Interface::USB => {
                cx.resources.usb_midi.lock(
                    |usb_midi| if let Err(e) = usb_midi.transmit(packet) {
                        rprintln!("Failed to send USB MIDI: {:?}", e)
                    }
                );
            }
            Interface::Serial(_) => {
                // TODO use proper serial port #
                cx.resources.serial_midi.lock(
                    |serial_out| if let Err(e) = serial_out.transmit(packet) {
                        rprintln!("Failed to send Serial MIDI: {:?}", e)
                    });
            }
            _ => {}
        }
    }

    extern "C" {
        // Reuse some interrupts for software task scheduling.
        fn EXTI0();
        fn EXTI1();
        fn USART1();
    }
};
