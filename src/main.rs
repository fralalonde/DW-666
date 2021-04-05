#![no_main]
#![no_std]
#![feature(slice_as_chunks)]
#![feature(alloc_error_handler)]

#[macro_use]
extern crate enum_map;

#[macro_use]
extern crate rtt_target;

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

use embedded_hal::digital::v2::OutputPin;
use rtic::app;
use rtic::cyccnt::U32Ext as _;

use ssd1306::{prelude::*, Builder, I2CDIBuilder};

use usb_device::bus;

use cortex_m::asm::delay;

use input::{Scan, Controls};

use midi::{SerialMidi, MidiClass, Packet, CableNumber, usb_device, Note, Channel, Velocity, Transmit, Receive, RouteBinding, UsbMidi};
use midi::{Interface, Message};
use midi::RouteBinding::*;
use core::result::Result;
use hashbrown::HashMap;

use panic_rtt_target as _;
use core::convert::TryFrom;
// use crate::app::AppState;
// use crate::clock::{CPU_FREQ, PCLK1_FREQ};

// renamed for RTIC genericity
use stm32f4xx_hal::stm32 as device;

use hal::{gpio::AlternateOD, i2c::I2c};

// STM32 universal (?)
use hal::{
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
};

const CTL_SCAN: u32 = 100_000;
const LED_BLINK_CYCLES: u32 = 15_400_000;
const ARP_NOTE_LEN: u32 = 7200000;

static mut USB_EP_MEMORY: [u32; 1024] = [0; 1024];

extern crate alloc;
// use self::alloc::vec;
use core::alloc::Layout;
use cortex_m::asm;
// use cortex_m_rt::entry;
// this is the allocator the application will use
// use alloc_cortex_m::CortexMHeap;
// #[global_allocator]
// static ALLOCATOR: CortexMHeap = CortexMHeap::empty();
//
// const HEAP_SIZE: usize = 96 * 1024; // in bytes

// define what happens in an Out Of Memory (OOM) condition
#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    asm::bkpt();
    loop {}
}

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};

const FAST_HEAP_SIZE: usize = 32 * 1024; // 32 KB
const HEAP_SIZE: usize = 64 * 1024; // 96KB
const LEAF_SIZE: usize = 16;

pub static mut FAST_HEAP: [u8; FAST_HEAP_SIZE] = [0u8; FAST_HEAP_SIZE];
pub static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

// This allocator can't work in tests since it's non-threadsafe.
#[cfg_attr(not(test), global_allocator)]
static ALLOC: NonThreadsafeAlloc = unsafe {
    let fast_param = FastAllocParam::new(FAST_HEAP.as_ptr(), FAST_HEAP_SIZE);
    let buddy_param = BuddyAllocParam::new(HEAP.as_ptr(), HEAP_SIZE, LEAF_SIZE);
    NonThreadsafeAlloc::new(fast_param, buddy_param)
};

// // fast alloc big stack
// use core::mem::MaybeUninit;
// const STACK_SIZE: usize = 64 * 1024;
// const NTHREADS: usize = 1;
// #[link_section = ".uninit.STACKS"]
// static mut STACKS: MaybeUninit<[[u8; STACK_SIZE]; NTHREADS]> = MaybeUninit::uninit();

#[app(device = crate::device, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        // clock: rtc::RtcClock,
        on_board_led: PC13<Output<PushPull>>,
        controls: input::Controls<PA6<Input<PullUp>>, PA7<Input<PullUp>>>,
        app_state: app::AppState,
        display: output::Display,
        // midi_router: midi::Router,
        usb_midi: midi::UsbMidi,
        serial_midi: SerialMidi,
    }

    #[init(schedule = [led_blink, control_scan])]
    fn init(mut cx: init::Context) -> init::LateResources {
        // for some RTIC reason statics need to go first
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        rtt_init_print!();

        rprintln!("Initializing");

        // unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }
        rprintln!("Allocator OK");

        // Initialize (enable) the monotonic timer (CYCCNT)
        cx.core.DCB.enable_trace();
        // required on Cortex-M7 devices that software lock the DWT (e.g. STM32F7)
        cx.core.DWT.enable_cycle_counter();

        let peripherals = cx.device;

        let rcc = peripherals.RCC.constrain();

        let clocks = rcc
            .cfgr
            .sysclk(100.mhz())
            .freeze();

        let gpioa = peripherals.GPIOA.split();
        let gpiob = peripherals.GPIOB.split();
        let gpioc = peripherals.GPIOC.split();

        rprintln!("Clocks OK");

        rprintln!("RTC OK");

        // // Setup LED
        let mut on_board_led = gpioc
            .pc13
            .into_push_pull_output();
        on_board_led.set_low().unwrap();
        cx.schedule.led_blink(cx.start + LED_BLINK_CYCLES.cycles(), true).unwrap();

        rprintln!("Blinker OK");

        // Setup Encoders
        let encoder = input::encoder(
            event::RotaryId::MAIN,
            gpioa.pa6.into_pull_up_input(),
            gpioa.pa7.into_pull_up_input(),
        );
        // let _enc_push = gpioa.pa5.into_pull_down_input(&mut gpioa.crl);
        let controls = Controls::new(encoder);
        cx.schedule.control_scan(cx.start + CTL_SCAN.cycles()).unwrap();
        rprintln!("Controls OK");

        // Setup Display
        let scl = gpiob.pb8.into_alternate_af4().set_open_drain();
        let sda = gpiob.pb9.into_alternate_af4().set_open_drain();

        let i2c = I2c::i2c1(peripherals.I2C1, (scl, sda), 400.khz(), clocks);
        let interface = I2CDIBuilder::new().init(i2c);
        let mut oled: GraphicsMode<_> = Builder::new().connect(interface).into();
        oled.init().unwrap();

        output::draw_logo(&mut oled);

        rprintln!("Screen OK");

        // Configure serial
        let tx_pin = gpioa.pa2.into_alternate_af7();
        let rx_pin = gpioa.pa3.into_alternate_af7();

        // Configure Midi
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
        delay(60000);

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
        // USB devices MUST init after classes
        let usb_dev = usb_device(usb_bus);
        rprintln!("USB OK");
        let mut map: HashMap<u32, u64> = HashMap::new();
        map.insert(4,8);
        map.get(&5);

        let mut midi_router: midi::Router = midi::Router::new();
        // // TODO add default routes here
        let _usb_echo = midi_router.bind(midi::Route::echo(Interface::USB));
        // rprintln!("Routes OK");

        rprintln!("-> Initialized");

        init::LateResources {
            // clock,
            controls,
            on_board_led,
            app_state: app::AppState::default(),
            // midi_router,
            display: output::Display {
                oled,
            },
            usb_midi: midi::UsbMidi {
                dev: usb_dev,
                midi_class,
            },
            serial_midi,
        }
    }

    /// RTIC defaults to SLEEP_ON_EXIT on idle, which is very eco-friendly (SUCH WATTAGE)
    /// Except that sleeping FUCKS with RTT logging, debugging, etc (WOW)
    /// Override this with a puny idle loop (MUCH WASTE)
    #[allow(clippy::empty_loop)]
    #[idle(/*resources = [on_board_led]*/)]
    fn idle(cx: idle::Context) -> ! {
        // let mut led = cx.resources.on_board_led;
        // let mut led_on = true;
        loop {
            delay(35_000_000);
            // if led_on {
            //     led.lock(|led| led.set_high().unwrap());
            // } else {
            //     led.lock(|led| led.set_low().unwrap());
            // }
            // led_on = !led_on;
        }
    }

    /// USB receive interrupt
    #[task(binds = OTG_FS, spawn = [dispatch_from], resources = [usb_midi], priority = 3)]
    fn usb_interrupt(cx: usb_interrupt::Context) {
        // poll() is _required_ here else receive() might block forever!
        if cx.resources.usb_midi.poll() {
            while let Some(packet) = cx.resources.usb_midi.receive().unwrap() {
                cx.spawn.dispatch_from(Interface::USB, packet);
            }
        }
    }

    /// Serial receive interrupt
    #[task(binds = USART2, spawn = [dispatch_from], resources = [serial_midi], priority = 3)]
    fn serial_irq0(cx: serial_irq0::Context) {
        if let Err(_err) = cx.resources.serial_midi.flush() {
            // TODO record transmission error
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

    #[task(resources = [on_board_led], schedule = [led_blink])]
    fn led_blink(cx: led_blink::Context, led_on: bool) {
        let led = cx.resources.on_board_led;
        if led_on {
            led.set_high().unwrap();
        } else {
            led.set_low().unwrap();
        }
        cx.schedule.led_blink(cx.scheduled + LED_BLINK_CYCLES.cycles(), !led_on).unwrap();
    }

    #[task(spawn = [send_midi], schedule = [send_midi], /*resources = [midi_router],*/ priority = 3)]
    fn dispatch_from(cx: dispatch_from::Context, from: Interface, packet: Packet) {
        // let mut router: &mut midi::Router = cx.resources.midi_router;
        // router.dispatch_from(cx.scheduled, packet, from, cx.spawn, cx.schedule)

        match (from, packet) {
            (Interface::USB, packet) => {
                // echo USB packets
                cx.spawn.send_midi(Interface::USB, packet);
                cx.spawn.send_midi(Interface::Serial(0), packet);
            }
            (Interface::Serial(_), packet) => {
                if let Ok(message) = Message::try_from(packet) {
                    match message {
                        Message::SysexBegin(byte1, byte2) => rprint!("Sysex [ 0x{:x}, 0x{:x}", byte1, byte2),
                        Message::SysexCont(byte1, byte2, byte3) => rprint!(", 0x{:x}, 0x{:x}, 0x{:x}", byte1, byte2, byte3),
                        Message::SysexEnd => rprintln!(" ]"),
                        Message::SysexEnd1(byte1) => rprintln!(", 0x{:x} ]", byte1),
                        Message::SysexEnd2(byte1, byte2) => rprintln!(", 0x{:x}, 0x{:x} ]", byte1, byte2),
                        message => rprintln!("{:?}", message)
                    }
                }
                cx.spawn.send_midi(Interface::Serial(0), packet);
            }
            (_, _) => {}
        }
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
