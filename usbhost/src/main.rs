#![no_std]
#![no_main]

#[macro_use]
extern crate rtt_target;
use panic_rtt_target as _;
// use rtt_target::{rprintln, rtt_init_print};

use trinket_m0 as hal;

use hal::clock::GenericClockController;
use hal::entry;
use hal::pac::{interrupt, CorePeripherals, Peripherals};

use hal::usb::UsbBus;

use usb_device::prelude::*;

use cortex_m::asm::delay as cycle_delay;
use cortex_m::peripheral::NVIC;
use atsamd_hal::time::Hertz;
use atsamd_usb_host::{SAMDHost, Pins};
use crate::midihost::MidiHost;
use usb_host::Driver;

mod midihost;

// static mut USB_ALLOCATOR: Option<UsbBusAllocator<UsbBus>> = None;
// static mut USB_BUS: Option<UsbDevice<UsbBus>> = None;
// static mut USB_SERIAL: Option<SerialPort<UsbBus>> = None;

#[entry]
fn main() -> ! {

    // let mut peripherals = Peripherals::take().unwrap();
    // let mut core = CorePeripherals::take().unwrap();
    //
    //
    // let mut clocks = GenericClockController::with_internal_32kosc(
    //     peripherals.GCLK,
    //     &mut peripherals.PM,
    //     &mut peripherals.SYSCTRL,
    //     &mut peripherals.NVMCTRL,
    // );

    let mut peripherals = Peripherals::take().unwrap();
        let mut core = CorePeripherals::take().unwrap();
    let mut pins = hal::Pins::new(peripherals.PORT);
    let mut clocks = GenericClockController::with_internal_32kosc(
        peripherals.GCLK,
        &mut peripherals.PM,
        &mut peripherals.SYSCTRL,
        &mut peripherals.NVMCTRL,
    );
    rtt_init_print!();
    rprintln!("Initializing");

    let mut red_led = pins.d13.into_open_drain_output(&mut pins.port);

    let serial = hal::uart(
        &mut clocks,
        Hertz(115200),
        peripherals.SERCOM0,
        &mut peripherals.PM,
        pins.d3.into_floating_input(&mut pins.port),
        pins.d4.into_floating_input(&mut pins.port),
        &mut pins.port,
    );

    // let bus_allocator = unsafe {
    //     USB_ALLOCATOR = Some(hal::usb_allocator(
    //         peripherals.USB,
    //         &mut clocks,
    //         &mut peripherals.PM,
    //         pins.usb_dm,
    //         pins.usb_dp,
    //         &mut pins.port,
    //     ));
    //     USB_ALLOCATOR.as_ref().unwrap()
    // };
    //
    // unsafe {
    //     USB_SERIAL = Some(SerialPort::new(&bus_allocator));
    //     USB_BUS = Some(
    //         UsbDeviceBuilder::new(&bus_allocator, UsbVidPid(0x16c0, 0x27dd))
    //             .manufacturer("Fake company")
    //             .product("Serial port")
    //             .serial_number("TEST")
    //             .device_class(USB_CLASS_CDC)
    //             .build(),
    //     );
    // }

    let usb_pins = Pins::new(
        pins.usb_dm.into_floating_input(&mut pins.port),
        pins.usb_dp.into_floating_input(&mut pins.port),
        Some(pins.usb_sof.into_floating_input(&mut pins.port)),
        Some(pins.usb_host_enable.into_floating_input(&mut pins.port)),
    );

    let (mut usb_host, _millis) = SAMDHost::new(
        peripherals.USB,
        usb_pins,
        &mut pins.port,
        &mut clocks,
        &mut peripherals.PM,
        &|| 0,
    );

    let mut midi_host = MidiHost::new(|_, _| {});

    unsafe {
        core.NVIC.set_priority(interrupt::USB, 1);
        NVIC::unmask(interrupt::USB);
    }

    // Flash the LED in a spin loop to demonstrate that USB is entirely interrupt driven
    loop {
        cycle_delay(2 * 1024 * 1024);
        red_led.toggle();
        // rprintln!("asdfsdfsdfsde");
        if let Err(e)  = midi_host.tick(0, &mut usb_host) {
            rprintln!("MIDI host error: {:?}", e);
        }
    }
}

#[interrupt]
fn USB() {
    HANDLERS.call(0);
}

// #[interrupt]
// fn USB() {
//     // unsafe {
//     //     USB_BUS.as_mut().map(|usb_dev| {
//     //         USB_SERIAL.as_mut().map(|serial| {
//     //             usb_dev.poll(&mut [serial]);
//     //             let mut buf = [0u8; 64];
//     //
//     //             if let Ok(count) = serial.read(&mut buf) {
//     //                 for (i, c) in buf.iter().enumerate() {
//     //                     if i >= count {
//     //                         break;
//     //                     }
//     //                     serial.write(&[c.clone()]).ok();
//     //                 }
//     //             };
//     //         });
//     //     });
//     // };
// }

// #[interrupt]
// fn SERCOM0_0() {
//     // Data Register Empty interrupt.
//     unsafe {
//         $global_name.as_mut().map(|wifi| {
//             wifi._handle_data_empty();
//         });
//     }
// }
//
// #[interrupt]
// fn SERCOM0_2() {
//     // Recieve Complete interrupt.
//     unsafe {
//         $global_name.as_mut().map(|wifi| {
//             wifi._handle_rx();
//         });
//     }
// }

// #![no_main]
// #![no_std]
//
// use trinket_m0 as hal;
//
// use hal::clock::GenericClockController;
// use hal::delay::Delay;
// use hal::gpio::*;
// use hal::prelude::*;
// use hal::sercom::*;
// use hal::time::Hertz;
// use hal::timer::TimerCounter;
// use nb::block;
//
// #[rtic::app(device = hal::pac, peripherals = true)]
// const APP: () = {
//     struct Resources {
//         serial: UART0<Sercom0Pad3<Pa7<PfD>>, Sercom0Pad2<Pa6<PfD>>, (), ()>,
//     }
//
//     #[init]
//     fn init(context: init::Context) -> init::LateResources {
//         let mut p = context.device;
//
//         let mut clocks = GenericClockController::with_internal_32kosc(
//             p.GCLK,
//             &mut p.PM,
//             &mut p.SYSCTRL,
//             &mut p.NVMCTRL,
//         );
//
//         let mut pins = crate::hal::Pins::new(p.PORT);
//         let delay = Delay::new(context.core.SYST, &mut clocks);
//
//         let (rx, tx) = (
//             pins.d3.into_floating_input(&mut pins.port),
//             pins.d4.into_floating_input(&mut pins.port),
//         );
//
//         let serial = hal::uart(
//             &mut clocks,
//             Hertz(9600),
//             p.SERCOM0,
//             &mut p.PM,
//             rx,
//             tx,
//             &mut pins.port,
//         );
//
//         init::LateResources {
//             serial,
//         }
//     }
//
//     #[idle(resources = [serial])]
//     fn idle(c: idle::Context) -> ! {
//         // // Matching resources in c3_display
//         // // Half the tail length, since half the leds per m
//         // let mut elements = Elements::new(80, 8);
//         // // Chosen by fair dice roll
//         // let mut rand = oorandom::Rand32::new(0);
//         // // On average add a new color every 15 steps
//         // let mut steps = rand.rand_range(10..20);
//         // // Do something when host isn't active yet
//         // // Drops first byte
//         // while c.resources.serial.read().is_err() {
//         //     steps -= 1;
//         //     if steps == 0 {
//         //         steps = rand.rand_range(10..20);
//         //         elements
//         //             .add_predefined(rand.rand_range(0..c3_led_tail::COLORS.len() as u32) as usize)
//         //             .unwrap();
//         //     }
//         //     block!(c.resources.timer.wait()).unwrap();
//         // }
//         // // Host driven mode
//         // loop {
//         //     if let Ok(byte) = c.resources.serial.read().map(|x| x as usize) {
//         //         if byte < c3_led_tail::COLORS.len() {
//         //             elements.add_predefined(byte).unwrap();
//         //         }
//         //     }
//         //     if c.resources.timer.wait().is_ok() {
//         //         elements.step();
//         //         c.resources
//         //             .dotstar
//         //             // Only the onboard led
//         //             .write(smart_leds::gamma(elements.iter()).take(1))
//         //             .expect("Write");
//         //         c.resources
//         //             .external
//         //             // Only the onboard led
//         //             .write(smart_leds::gamma(elements.iter()))
//         //             .expect("Write");
//         //     }
//         // }
//         loop {}
//     }
// };
