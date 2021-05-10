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
        AF4,
        gpioa::{PA6, PA7},
        gpioc::PC13, GpioExt, Input, Output,
        PullUp,
        PushPull,
    },
    i2c::I2c,
    otg_fs::{USB, UsbBus, UsbBusType},
    rcc::RccExt,
    serial::{self, config::StopBits, Rx, Serial, Tx},
    stm32 as device,
    stm32::Peripherals,
    stm32::USART2,
    time::U32Ext,
    timer::Timer,
};
use ssd1306::{prelude::*, Builder, I2CDIBuilder};

use hal::prelude::_embedded_hal_digital_v2_OutputPin;
use panic_rtt_target as _;
use rtic::app;
use rtic::cyccnt::U32Ext as _;
use usb_device::bus;

use midi::{CableNumber, MidiClass, Route, SerialMidi, usb_device};
use midi::{Interface, Packet, Receive, Transmit};

use crate::clock::long_now;
use crate::devices::sequential::    evolver;
use crate::apps::dw6000_control::Dw6000Control;
use crate::midi::{channel, event_print, Service};
use alloc::string::String;

mod event;
mod clock;
mod midi;
mod devices;
mod apps;
mod output;

pub const CPU_FREQ: u32 = 96_000_000;
pub const CYCLES_PER_MICROSEC: u32 = CPU_FREQ / 1_000_000;
pub const CYCLES_PER_MILLISEC: u32 = CPU_FREQ / 1_000;

const LED_BLINK: u32 = CPU_FREQ / 4;
const CLOCK_TICK: u32 = CPU_FREQ / 1024;

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

#[app(device = crate::device, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        chaos: nanorand::WyRand,
        on_board_led: PC13<Output<PushPull>>,
        display: output::Display,
        midi_router: midi::Router,
        usb_midi: midi::UsbMidi,
        serial_midi: SerialMidi,
    }

    #[init(schedule = [timer_task])]
    fn init(mut cx: init::Context) -> init::LateResources {
        // RTIC needs statics to go first
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        rtt_init_print!();
        rprintln!("Initializing");

        let peripherals = cx.device;
        let rcc = peripherals.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(CPU_FREQ.hz()).freeze();

        cx.core.DCB.enable_trace();
        cx.core.DWT.enable_cycle_counter();

        let gpioa = peripherals.GPIOA.split();
        let gpiob = peripherals.GPIOB.split();
        let gpioc = peripherals.GPIOC.split();

        let on_board_led = gpioc.pc13.into_push_pull_output();

        // Setup Display
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

        let chaos = nanorand::WyRand::new_seed(0);

        let mut midi_router: midi::Router = midi::Router::default();
        // let _usb_echo = midi_router.bind(Route::echo(Interface::USB).filter(event_print()));
        // let _serial_print = midi_router.bind(Route::from(Interface::Serial(0)).filter(event_print()));

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

        let mut dwctrl = Dw6000Control::new((Interface::Serial(0), channel(1)), (Interface::USB, channel(1)));
        dwctrl.start(cx.start, &mut midi_router, cx.schedule);

        rprintln!("Routes OK");

        rprintln!("-> Initialized");

        init::LateResources {
            chaos,
            on_board_led,
            display: output::Display {
                oled,
            },
            midi_router,
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
                cx.spawn.dispatch_from(Interface::USB, packet).unwrap();
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
            cx.spawn.dispatch_from(Interface::Serial(0), packet).unwrap();
        }
    }


    /// Serial receive interrupt
    #[task(resources = [chaos], spawn = [send_midi], schedule = [timer_task], priority = 3)]
    fn timer_task(mut cx: timer_task::Context, mut task: clock::TimerTask) {
        let resources = &mut cx.resources;
        let spawn = &mut cx.spawn;
        if let Some(next_iter_delay) = (task)(resources, spawn) {
            cx.schedule.timer_task(cx.scheduled + next_iter_delay, task);
        }
    }

    #[task(spawn = [send_midi, redraw], schedule = [timer_task], resources = [midi_router], priority = 3)]
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

    #[task(resources = [display])]
    fn redraw(ctx: redraw::Context, text: String) {
        ctx.resources.display.print(text)
    }

    extern "C" {
        // Reuse some interrupts for software task scheduling.
        fn EXTI0();
        fn EXTI1();
        fn USART1();
    }
};
