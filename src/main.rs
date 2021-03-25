#![no_main]
#![no_std]
// #![feature(alloc_error_handler)]
#![feature(slice_as_chunks)]

#[macro_use]
extern crate enum_map;

#[macro_use]
extern crate rtt_target;

extern crate cortex_m;

mod event;
// mod rtc;
mod clock;
mod input;
mod midi;
mod output;
mod app;

mod device;

use embedded_hal::digital::v2::OutputPin;
use rtic::app;
use rtic::cyccnt::U32Ext as _;

use stm32f1xx_hal::gpio::{State, Input, PullUp, Output, PushPull};
use stm32f1xx_hal::i2c::{BlockingI2c, DutyCycle, Mode};
use stm32f1xx_hal::prelude::*;

use ssd1306::{prelude::*, Builder, I2CDIBuilder};
use stm32f1xx_hal::usb::{Peripheral, UsbBus, UsbBusType};
use stm32f1xx_hal::device::USART2;


use usb_device::bus;

use cortex_m::asm::delay;

use input::{Scan, Controls};

use midi::{SerialIn, SerialOut, MidiClass, Packet, CableNumber, usb_device, Note, Channel, Velocity, Transmit, Receive};
use core::result::Result;
use stm32f1xx_hal::serial;

use panic_rtt_target as _;
use stm32f1xx_hal::serial::{Rx, StopBits};
use stm32f1xx_hal::gpio::gpioa::{PA6, PA7};
use core::convert::TryFrom;
use crate::app::AppState;
use crate::clock::{CPU_FREQ, PCLK1_FREQ};
use crate::event::{Endpoint, MidiLane};
use stm32f1xx_hal::gpio::gpioc::PC13;
use crate::midi::Message;
use crate::event::MidiLane::{Src, Dst, Route};

const CTL_SCAN: u32 = 7200;
const LED_BLINK_CYCLES: u32 = 14_400_000;
const ARP_NOTE_LEN: u32 = 7200000;

#[app(device = stm32f1xx_hal::pac, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
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
    fn init(ctx: init::Context) -> init::LateResources {
        // for some RTIC reason statics need to go first
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        rtt_init_print!();

        // unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }
        // rprintln!("Allocator OK");

        // Enable cycle counter
        let mut core = ctx.core;
        core.DWT.enable_cycle_counter();

        let peripherals: stm32f1xx_hal::stm32::Peripherals = ctx.device;

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

        // Setup RTC
        // let mut pwr = peripherals.PWR;
        // let mut backup_domain = rcc.bkp.constrain(peripherals.BKP, &mut rcc.apb1, &mut pwr);
        // let rtc = Rtc::rtc(peripherals.RTC, &mut backup_domain);
        // let clock = rtc::RtcClock::new(rtc);

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
        ctx.schedule.led_blink(ctx.start + LED_BLINK_CYCLES.cycles(), true).unwrap();

        rprintln!("Blinker OK");

        // let mut timer3 = Timer::tim3(peripherals.TIM3, &clocks, &mut rcc.apb1)
        //     .start_count_down(input::SCAN_FREQ_HZ.hz());
        // timer3.listen(Event::Update);

        // Setup Encoders
        let encoder = input::encoder(
            event::RotaryId::MAIN,
            gpioa.pa6.into_pull_up_input(&mut gpioa.crl),
            gpioa.pa7.into_pull_up_input(&mut gpioa.crl),
        );
        // let _enc_push = gpioa.pa5.into_pull_down_input(&mut gpioa.crl);
        let controls = Controls::new(encoder);

        ctx.schedule.control_scan(ctx.start + CTL_SCAN.cycles()).unwrap();

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
        // USB device MUST init after classes
        let usb_dev = usb_device(USB_BUS.as_ref().unwrap());
        rprintln!("USB OK");

        // Setup Arp
        // ctx.schedule.arp_note_on(ctx.start + ARP_NOTE_LEN.cycles()).unwrap();
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
    #[idle]
    fn idle(_ctx: idle::Context) -> ! {
        loop {}
    }

    /// USB transmit interrupt
    #[task(binds = USB_HP_CAN_TX, resources = [usb_midi], priority = 3)]
    fn usb_hp_can_tx(ctx: usb_hp_can_tx::Context) {
        let _unhandled = ctx.resources.usb_midi.poll();
    }

    /// USB receive interrupt
    #[task(binds = USB_LP_CAN_RX0, spawn = [dispatch_midi], resources = [usb_midi], priority = 3)]
    fn usb_lp_can_rx0(ctx: usb_lp_can_rx0::Context) {
        // poll() is required else receive() might block forever
        if ctx.resources.usb_midi.poll() {
            while let Some(packet) = ctx.resources.usb_midi.receive().unwrap() {
                ctx.spawn.dispatch_midi(Src(Endpoint::USB), packet);
            }
        }
    }

    /// Serial receive interrupt
    #[task(binds = USART2, spawn = [dispatch_midi], resources = [serial_midi_in, serial_midi_out], priority = 3)]
    fn serial_irq0(ctx: serial_irq0::Context) {
        if let Err(_err) = ctx.resources.serial_midi_out.flush() {
            // TODO record transmission error
        }

        while let Ok(Some(packet)) = ctx.resources.serial_midi_in.receive() {
            ctx.spawn.dispatch_midi(Src(Endpoint::Serial(0)), packet);
        }
    }

    /// Encoder scan timer interrupt
    #[task(resources = [controls], spawn = [dispatch_ctl], schedule = [control_scan], priority = 1)]
    fn control_scan(ctx: control_scan::Context) {
        let controls = ctx.resources.controls;
        if let Some(event) = controls.scan(clock::long_now()) {
            ctx.spawn.dispatch_ctl(event).unwrap();
        }
        ctx.schedule.control_scan(ctx.scheduled + CTL_SCAN.cycles()).unwrap();
    }

    #[task(spawn = [dispatch_ctl, dispatch_app], resources = [controls, app_state], capacity = 5, priority = 1)]
    fn dispatch_ctl(ctx: dispatch_ctl::Context, event: event::CtlEvent) {
        if let Some(derived) = ctx.resources.controls.derive(event) {
            ctx.spawn.dispatch_ctl(derived);
        }
        if let Some(app_change) = ctx.resources.app_state.dispatch_ctl(event) {
            ctx.spawn.dispatch_app(app_change);
        }
    }

    #[task(resources = [display], capacity = 5, priority = 1)]
    fn dispatch_app(ctx: dispatch_app::Context, event: event::AppEvent) {
        // TODO filter conditional output spawn
        ctx.resources.display.update(event)
    }

    #[task(resources = [app_state], spawn = [dispatch_midi], schedule = [arp_note_off, arp_note_on])]
    fn arp_note_on(ctx: arp_note_on::Context) {
        let app_state: &mut AppState = ctx.resources.app_state;

        let channel = app_state.arp.channel;
        let note = app_state.arp.note;
        // let velo = Velocity::try_from().unwrap();
        app_state.arp.bump();

        let note_on = midi::note_on(app_state.arp.channel, app_state.arp.note, 0x7F).unwrap();
        ctx.spawn.dispatch_midi(Route(0), note_on.into()).unwrap();

        ctx.schedule.arp_note_off(ctx.scheduled + ARP_NOTE_LEN.cycles(), channel, note).unwrap();
        ctx.schedule.arp_note_on(ctx.scheduled + ARP_NOTE_LEN.cycles()).unwrap();
    }

    #[task(spawn = [dispatch_midi], capacity = 2)]
    fn arp_note_off(ctx: arp_note_off::Context, channel: Channel, note: Note) {
        let note_off = midi::Message::NoteOff(channel, note, Velocity::try_from(0).unwrap());
        ctx.spawn.dispatch_midi(Route(0), note_off.into()).unwrap();
    }

    #[task(resources = [on_board_led], schedule = [led_blink])]
    fn led_blink(ctx: led_blink::Context, led_on: bool) {
        if led_on {
            ctx.resources.on_board_led.set_high().unwrap();
        } else {
            ctx.resources.on_board_led.set_low().unwrap();
        }
        ctx.schedule.led_blink(ctx.scheduled + LED_BLINK_CYCLES.cycles(), !led_on).unwrap();
    }

    #[task(spawn = [dispatch_midi, send_serial_midi], resources = [usb_midi], priority = 3)]
    fn dispatch_midi(ctx: dispatch_midi::Context, lane: MidiLane, packet: Packet) {
        match (lane, packet) {
            (Src(Endpoint::USB), packet) => {
                // echo USB packets
                ctx.spawn.dispatch_midi(Dst(Endpoint::USB), packet);
                ctx.spawn.dispatch_midi(Dst(Endpoint::Serial(0)), packet);
            }
            (Dst(Endpoint::USB), packet) => {
                // immediate forward
                if let Err(e) = ctx.resources.usb_midi.transmit(packet) {
                    rprintln!("Failed to send USB MIDI: {:?}", e)
                }
            }
            (Src(Endpoint::Serial(_)), packet) => {
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
            (Dst(Endpoint::Serial(_)), packet) => {
                ctx.spawn.send_serial_midi(packet);
            }
            (Route(_), _) => {}
        }
    }

    /// Sending Serial MIDI is a slow, _blocking_ operation (for now?).
    /// Use lower priority and enable queuing of tasks (capacity > 1).
    #[task(capacity = 16, priority = 2, resources = [serial_midi_out])]
    fn send_serial_midi(mut ctx: send_serial_midi::Context, packet: Packet) {
        rprintln!("Send Serial MIDI: {:?}", packet);
        ctx.resources.serial_midi_out.lock(
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
