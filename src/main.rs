#![no_main]
#![no_std]
#![feature(slice_as_chunks)]

#[macro_use]
extern crate enum_map;

#[macro_use]
extern crate rtt_target;

// TODO reenable later: "#[derive(Hash)] doesn't currently support `enum` and `union`"
// #[macro_use]
// extern crate hash32_derive;

extern crate cortex_m;

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

use midi::{SerialIn, SerialOut, MidiClass, Packet, CableNumber, usb_device, Note, Channel, Velocity, Transmit, Receive, Binding};
use midi::Binding::*;
use core::result::Result;

use panic_rtt_target as _;
use core::convert::TryFrom;
use crate::app::AppState;
use crate::clock::{CPU_FREQ, PCLK1_FREQ};

// STM32F1 specific
extern crate stm32f1xx_hal as hal;
use hal::i2c::{BlockingI2c, DutyCycle, Mode};
use hal::usb::{Peripheral, UsbBus, UsbBusType};
use hal::serial::StopBits;
use hal::gpio::State;

// STM32F4 specific
// extern crate stm32f4xx_hal as hal;
// use hal::i2c::I2c;
// use hal::otg_fs::{UsbBusType, UsbBus};
// use hal::serial::config::StopBits;

// STM32 universal (?)
use hal::{
    prelude::*,
    prelude::*,
    serial::{self, Serial, Rx, Tx},
    stm32::USART2,
    stm32::Peripherals,
    gpio::{
        Input, PullUp, Output, PushPull,
        gpioa::{PA6, PA7},
        gpioc::{PC13},
    },
};

use crate::midi::{Interface, Message};

const CTL_SCAN: u32 = 7200;
const LED_BLINK_CYCLES: u32 = 14_400_000;
const ARP_NOTE_LEN: u32 = 7200000;

#[app(device = hal::stm32, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        // clock: rtc::RtcClock,
        on_board_led: PC13<Output<PushPull>>,
        controls: input::Controls<PA6<Input<PullUp>>, PA7<Input<PullUp>>>,
        app_state: app::AppState,
        display: output::Display,
        usb_midi: midi::UsbMidi,
        serial_midi_in: SerialIn<Rx<USART2>>,
        serial_midi_out: SerialOut,
    }

    #[init(schedule = [led_blink, control_scan, arp_note_on])]
    fn init(cx: init::Context) -> init::LateResources {
        // for some RTIC reason statics need to go first
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        rtt_init_print!();

        // unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }
        // rprintln!("Allocator OK");

        // Enable cycle counter
        let mut core = cx.core;
        core.DWT.enable_cycle_counter();

        let peripherals: stm32f1xx_hal::stm32::Peripherals = cx.device;

        // Setup clocks
        let mut flash = peripherals.FLASH.constrain();
        let mut rcc = peripherals.RCC.constrain();
        let mut afio = peripherals.AFIO.constrain(&mut rcc.apb2);
        let clocks = rcc
            .cfgr
            .use_hse(8.mhz())
            // maximum CPU overclock
            .sysclk(CPU_FREQ.hz())
            .pclk1(PCLK1_FREQ.hz())
            .freeze(&mut flash.acr);

        assert!(clocks.usbclk_valid());

        rprintln!("Clocks OK");

        rprintln!("RTC OK");

        // Get GPIO busses
        let mut gpioa = peripherals.GPIOA.split(&mut rcc.apb2);
        let mut gpiob = peripherals.GPIOB.split(&mut rcc.apb2);
        let mut gpioc = peripherals.GPIOC.split(&mut rcc.apb2);

        // // Setup LED
        let mut on_board_led = gpioc
            .pc13
            .into_push_pull_output_with_state(&mut gpioc.crh, State::Low);
        on_board_led.set_low().unwrap();
        cx.schedule.led_blink(cx.start + LED_BLINK_CYCLES.cycles(), true).unwrap();

        rprintln!("Blinker OK");

        // Setup Encoders
        let encoder = input::encoder(
            event::RotaryId::MAIN,
            gpioa.pa6.into_pull_up_input(&mut gpioa.crl),
            gpioa.pa7.into_pull_up_input(&mut gpioa.crl),
        );
        // let _enc_push = gpioa.pa5.into_pull_down_input(&mut gpioa.crl);
        let controls = Controls::new(encoder);

        cx.schedule.control_scan(cx.start + CTL_SCAN.cycles()).unwrap();

        rprintln!("Controls OK");

        // Setup Display
        let scl = gpiob.pb8.into_alternate_open_drain(&mut gpiob.crh);
        let sda = gpiob.pb9.into_alternate_open_drain(&mut gpiob.crh);

        let i2c = BlockingI2c::i2c1(
            peripherals.I2C1,
            (scl, sda),
            &mut afio.mapr,
            Mode::Fast {
                frequency: 400_000.hz(),
                duty_cycle: DutyCycle::Ratio2to1,
            },
            clocks,
            &mut rcc.apb1,
            1000,
            10,
            1000,
            1000,
        );
        let oled_i2c = I2CDIBuilder::new().init(i2c);
        let mut oled: GraphicsMode<_> = Builder::new().connect(oled_i2c).into();
        oled.init().unwrap();

        output::draw_logo(&mut oled);

        rprintln!("Screen OK");

        // Configure serial
        let tx_pin = gpioa.pa2.into_alternate_push_pull(&mut gpioa.crl);
        let rx_pin = gpioa.pa3;

        // Configure Midi
        let mut usart = serial::Serial::usart2(
            peripherals.USART2,
            (tx_pin, rx_pin),
            &mut afio.mapr,
            serial::Config::default()
                .baudrate(31250.bps())
                .stopbits(StopBits::STOP1)
                .parity_none(),
            clocks,
            &mut rcc.apb1,
        );
        let (tx, mut rx) = usart.split();
        rx.listen();
        let serial_midi_out = SerialOut::new(tx);
        let serial_midi_in = SerialIn::new(rx, CableNumber::MIN);

        rprintln!("Serial port OK");

        // force USB reset for dev mode (it's a Blue Pill thing)
        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low().unwrap();
        delay(clocks.sysclk().0 / 100);

        let usb = Peripheral {
            usb: peripherals.USB,
            pin_dm: gpioa.pa11,
            pin_dp: usb_dp.into_floating_input(&mut gpioa.crh),
        };

        *USB_BUS = Some(UsbBus::new(usb));
        let midi_class = MidiClass::new(USB_BUS.as_ref().unwrap());
        // USB devices MUST init after classes
        let usb_dev = usb_device(USB_BUS.as_ref().unwrap());
        rprintln!("USB OK");

        // Setup Arp
        // cx.schedule.arp_note_on(cx.start + ARP_NOTE_LEN.cycles()).unwrap();
        rprintln!("Arp OK");

        rprintln!("-> Initialized");

        init::LateResources {
            // clock,
            controls,
            on_board_led,
            app_state: app::AppState::default(),
            display: output::Display {
                oled,
            },
            usb_midi: midi::UsbMidi {
                dev: usb_dev,
                midi_class,
            },
            serial_midi_in,
            serial_midi_out,
        }
    }

    /// RTIC defaults to SLEEP_ON_EXIT on idle, which is very eco-friendly (SUCH WATTAGE)
    /// Except that sleeping FUCKS with RTT logging, debugging, etc (WOW)
    /// Override this with a puny idle loop (MUCH WASTE)
    #[allow(clippy::empty_loop)]
    #[idle(spawn = [dispatch_midi])]
    fn idle(cx: idle::Context) -> ! {
        loop {
            // cx.
        }
    }

    /// USB transmit interrupt
    #[task(binds = USB_HP_CAN_TX, resources = [usb_midi], priority = 3)]
    fn usb_hp_can_tx(cx: usb_hp_can_tx::Context) {
        let _unhandled = cx.resources.usb_midi.poll();
    }

    /// USB receive interrupt
    #[task(binds = USB_LP_CAN_RX0, spawn = [dispatch_midi], resources = [usb_midi], priority = 3)]
    fn usb_lp_can_rx0(cx: usb_lp_can_rx0::Context) {
        // poll() is required else receive() might block forever
        if cx.resources.usb_midi.poll() {
            while let Some(packet) = cx.resources.usb_midi.receive().unwrap() {
                cx.spawn.dispatch_midi(Src(Interface::USB), packet);
            }
        }
    }

    /// Serial receive interrupt
    #[task(binds = USART2, spawn = [dispatch_midi], resources = [serial_midi_in, serial_midi_out], priority = 3)]
    fn serial_irq0(cx: serial_irq0::Context) {
        if let Err(_err) = cx.resources.serial_midi_out.flush() {
            // TODO record transmission error
        }

        while let Ok(Some(packet)) = cx.resources.serial_midi_in.receive() {
            cx.spawn.dispatch_midi(Src(Interface::Serial(0)), packet);
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

    #[task(resources = [app_state], spawn = [dispatch_midi], schedule = [arp_note_off, arp_note_on])]
    fn arp_note_on(cx: arp_note_on::Context) {
        let app_state: &mut AppState = cx.resources.app_state;

        let channel = app_state.arp.channel;
        let note = app_state.arp.note;
        // let velo = Velocity::try_from().unwrap();
        app_state.arp.bump();

        let note_on = midi::note_on(app_state.arp.channel, app_state.arp.note, 0x7F).unwrap();
        cx.spawn.dispatch_midi(Dst(Interface::Serial(0)), note_on.into()).unwrap();

        cx.schedule.arp_note_off(cx.scheduled + ARP_NOTE_LEN.cycles(), channel, note).unwrap();
        cx.schedule.arp_note_on(cx.scheduled + ARP_NOTE_LEN.cycles()).unwrap();
    }

    #[task(spawn = [dispatch_midi], capacity = 2)]
    fn arp_note_off(cx: arp_note_off::Context, channel: Channel, note: Note) {
        let note_off = midi::Message::NoteOff(channel, note, Velocity::try_from(0).unwrap());
        cx.spawn.dispatch_midi(Dst(Interface::Serial(0)), note_off.into()).unwrap();
    }

    #[task(resources = [on_board_led], schedule = [led_blink])]
    fn led_blink(cx: led_blink::Context, led_on: bool) {
        if led_on {
            cx.resources.on_board_led.set_high().unwrap();
        } else {
            cx.resources.on_board_led.set_low().unwrap();
        }
        cx.schedule.led_blink(cx.scheduled + LED_BLINK_CYCLES.cycles(), !led_on).unwrap();
    }

    #[task(spawn = [dispatch_midi, send_serial_midi], resources = [usb_midi], priority = 3)]
    fn dispatch_midi(cx: dispatch_midi::Context, lane: Binding, packet: Packet) {
        match (lane, packet) {
            (Src(Interface::USB), packet) => {
                crate::burp(cx.spawn);
                // echo USB packets
                cx.spawn.dispatch_midi(Dst(Interface::USB), packet);
                cx.spawn.dispatch_midi(Dst(Interface::Serial(0)), packet);
            }
            (Dst(Interface::USB), packet) => {
                // immediate forward
                if let Err(e) = cx.resources.usb_midi.transmit(packet) {
                    rprintln!("Failed to send USB MIDI: {:?}", e)
                }
            }
            (Src(Interface::Serial(_)), packet) => {
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
            }
            (Dst(Interface::Serial(_)), packet) => {
                cx.spawn.send_serial_midi(packet);
            }
            (_, _) => {}
        }
    }

    /// Sending Serial MIDI is a slow, _blocking_ operation (for now?).
    /// Use lower priority and enable queuing of tasks (capacity > 1).
    #[task(capacity = 16, priority = 2, resources = [serial_midi_out])]
    fn send_serial_midi(mut cx: send_serial_midi::Context, packet: Packet) {
        rprintln!("Send Serial MIDI: {:?}", packet);
        cx.resources.serial_midi_out.lock(
            |serial_out| if let Err(e) = serial_out.transmit(packet) {
                rprintln!("Failed to send Serial MIDI: {:?}", e)
            });
    }

    extern "C" {
        // Reuse some interrupts for software task scheduling.
        fn DMA1_CHANNEL5();
        fn DMA1_CHANNEL6();
        fn DMA1_CHANNEL7();
    }

};


fn burp(spawn: dispatch_midi::Spawn) {
    spawn.dispatch_midi(Binding::Dst(Interface::Serial(0)), Packet::from(Message::SysexEmpty)).unwrap()
    // dispatch_midi::Spawm
    // APP::spawn()
}