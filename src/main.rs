#![no_main]
#![no_std]
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

mod devices;

use embedded_hal::digital::v2::OutputPin;
use rtic::app;
use rtic::cyccnt::U32Ext as _;

use ssd1306::{prelude::*, Builder, I2CDIBuilder};

use usb_device::bus;

use cortex_m::asm::delay;

use input::{Scan, Controls};

use midi::{SerialIn, SerialOut, MidiClass, Packet, CableNumber, usb_device, Note, Channel, Velocity, Transmit, Receive};
use core::result::Result;

use panic_rtt_target as _;
use core::convert::TryFrom;
use crate::app::AppState;
use crate::clock::{CPU_FREQ, PCLK1_FREQ};

// STM32F1 specific
// extern crate stm32f1xx_hal as hal;
// use hal::i2c::{BlockingI2c, DutyCycle, Mode};
// use hal::usb::{Peripheral, UsbBus, UsbBusType};
// use hal::serial::StopBits;
// use hal::gpio::State;

#[cfg(feature = "stm32f4xx")]
use stm32f4 as _;
#[cfg(feature = "stm32f4xx")]
use stm32f4xx_hal as hal;
#[cfg(feature = "stm32f4xx")]
use stm32f4xx_hal::stm32 as device;

#[cfg(feature = "stm32f4xx")]
use hal::{gpio::AlternateOD, i2c::I2c};

// #[cfg(feature = "stm32f7xx")]
// use stm32f7 as _;
// #[cfg(feature = "stm32f7xx")]
// use stm32f7xx_hal as hal;
// #[cfg(feature = "stm32f7xx")]
// use stm32f7xx_hal::device;
// #[cfg(feature = "stm32f7xx")]
// use rtic::export::DWT;
//
// #[cfg(feature = "stm32f7xx")]
// use hal::{
//     gpio::Alternate,
//     i2c::{BlockingI2c, Mode},
// };

// STM32 universal (?)
use hal::{
    serial::{self, Serial, Rx, Tx, config::StopBits},
    stm32::USART2,
    stm32::Peripherals,
    gpio::{
        Input, PullUp, Output, PushPull,
        gpioa::{PA6, PA7},
        gpioc::{PC13},
    },
    otg_fs::{UsbBusType, UsbBus, USB},
};


use crate::event::MidiLane::{Src, Dst, Route};
use crate::event::{Endpoint, MidiLane};
use crate::midi::Message;

use hal::{gpio::GpioExt, gpio::AF4, rcc::RccExt, time::U32Ext};


const CTL_SCAN: u32 = 7200;
const LED_BLINK_CYCLES: u32 = 14_400_000;
const ARP_NOTE_LEN: u32 = 7200000;

#[app(device = crate::device, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
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
        static mut EP_MEMORY: [u32; 1024] = [0; 1024];

        rtt_init_print!();

        // unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }
        // rprintln!("Allocator OK");

        // Initialize (enable) the monotonic timer (CYCCNT)
        ctx.core.DCB.enable_trace();
        // required on Cortex-M7 devices that software lock the DWT (e.g. STM32F7)
        #[cfg(feature = "stm32f7xx")]
            DWT::unlock();
        ctx.core.DWT.enable_cycle_counter();

        let peripherals: device::Peripherals = ctx.device;

        // init sensor

        #[cfg(feature = "stm32f4xx")]
            let rcc = peripherals.RCC.constrain();
        #[cfg(feature = "stm32f7xx")]
            let mut rcc = peripherals.RCC.constrain();

        #[cfg(feature = "stm32f4xx")]
            let clocks = rcc.cfgr.sysclk(50.mhz()).freeze();
        #[cfg(feature = "stm32f7xx")]
            let clocks = rcc.cfgr.sysclk(216.mhz()).freeze();

        #[cfg(feature = "stm32f4xx")]
            let gpioa = peripherals.GPIOA.split();
        #[cfg(feature = "stm32f4xx")]
            let gpiob = peripherals.GPIOB.split();
        #[cfg(feature = "stm32f4xx")]
            let gpioc = peripherals.GPIOC.split();
        #[cfg(feature = "stm32f7xx")]
            let gpioh = peripherals.GPIOH.split();

        rprintln!("Clocks OK");

        // Setup RTC
        // let mut pwr = peripherals.PWR;
        // let mut backup_domain = rcc.bkp.constrain(peripherals.BKP, &mut rcc.apb1, &mut pwr);
        // let rtc = Rtc::rtc(peripherals.RTC, &mut backup_domain);
        // let clock = rtc::RtcClock::new(rtc);

        rprintln!("RTC OK");

        // // Setup LED
        let mut on_board_led = gpioc
            .pc13
            .into_push_pull_output();
        on_board_led.set_low().unwrap();
        ctx.schedule.led_blink(ctx.start + LED_BLINK_CYCLES.cycles(), true).unwrap();

        rprintln!("Blinker OK");

        // let mut timer3 = Timer::tim3(peripherals.TIM3, &clocks, &mut rcc.apb1)
        //     .start_count_down(input::SCAN_FREQ_HZ.hz());
        // timer3.listen(Event::Update);

        // Setup Encoders
        let encoder = input::encoder(
            event::RotaryId::MAIN,
            gpioa.pa6.into_pull_up_input(),
            gpioa.pa7.into_pull_up_input(),
        );
        // let _enc_push = gpioa.pa5.into_pull_down_input(&mut gpioa.crl);
        let controls = Controls::new(encoder);

        ctx.schedule.control_scan(ctx.start + CTL_SCAN.cycles()).unwrap();

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
        let mut usart = Serial::usart2(
            peripherals.USART2,
            (tx_pin, rx_pin),
            serial::config::Config::default()
                .baudrate(31250.bps())
                .stopbits(StopBits::STOP1)
                .parity_none(),
            clocks,
        ).unwrap();
        let (tx, mut rx) = usart.split();
        rx.listen();
        let serial_midi_out = SerialOut::new(tx);
        let serial_midi_in = SerialIn::new(rx, CableNumber::MIN);

        rprintln!("Serial port OK");

        // force USB reset for dev mode (it's a Blue Pill thing)
        let mut usb_dp = gpioa.pa12.into_push_pull_output();
        usb_dp.set_low().unwrap();
        delay(clocks.sysclk().0 / 100);

        let usb = USB {
            usb_global: peripherals.OTG_FS_GLOBAL,
            usb_device: peripherals.OTG_FS_DEVICE,
            usb_pwrclk: peripherals.OTG_FS_PWRCLK,
            pin_dm: gpioa.pa11.into_alternate_af10(),
            pin_dp: gpioa.pa12.into_alternate_af10(),
        };

        *USB_BUS = Some(UsbBus::new(usb, unsafe { EP_MEMORY }));
        let midi_class = MidiClass::new(USB_BUS.as_ref().unwrap());
        // USB devices MUST init after classes
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
        fn EXTI0();
        fn EXTI1();
        fn USART1();
        // fn DMA1_CHANNEL5();
        // fn DMA1_CHANNEL6();
        // fn DMA1_CHANNEL7();
    }
};
