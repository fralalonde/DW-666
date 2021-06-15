#![no_std]
#![no_main]

// use trinket_m0 as hal;
//
// use atsamd_hal::common::gpio::{PfB};
//
// use hal::clock::GenericClockController;

use trinket_m0 as hal;

// use hal::pac::gclk::clkctrl::GEN_A;
// use hal::pac::gclk::genctrl::SRC_A;
// use hal::sercom::{Sercom0Pad2, Sercom0Pad3, UART0};

use hal::delay::Delay;
use hal::gpio::*;
use hal::prelude::*;
use hal::sercom::*;
use hal::time::Hertz;
use hal::timer::TimerCounter;

use nb::block;
use rtic::app;
use atsamd_hal::gpio::PfC;
use atsamd_hal::common::sercom::v2::Pad3;
use atsamd_hal::time::U32Ext;

macro_rules! dbgprint {
    ($($arg:tt)*) => {{}};
}

#[app(device = crate::hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        // blue_led: Pa17<Output<OpenDrain>>,
        // tx_led: Pa27<Output<OpenDrain>>,
        // rx_led: Pb3<Output<OpenDrain>>,
        uart: UART0<Sercom0Pad3<Pa7<PfD>>, Sercom0Pad2<Pa6<PfD>>, (), ()>,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        let mut device = cx.device;
        let mut core = cx.core;
        let mut  dp = cx.peripherals;

        let mut pins = hal::Pins::new(device.PORT);

        let mut clocks = GenericClockController::with_internal_32kosc(
            device.GCLK,
            &mut device.PM,
            &mut device.SYSCTRL,
            &mut device.NVMCTRL,
        );
        // clocks.configure_gclk_divider_and_source(GEN_A::GCLK2, 1, SRC_A::DFLL48M, false);
        // let gclk2 = clocks
        //     .get_gclk(GEN_A::GCLK2)
        //     .expect("Could not get clock 2");

        dbgprint!("Initializing serial port");

        // let mut led = pins.led.into_open_drain_output(&mut pins.port);
        // led.set_low().unwrap();

        let (/*odi, oci, nc, edi, eci, enc, */rx, tx) = (
            // Onboard apa102
            // pins.dotstar_di.into_push_pull_output(&mut pins.port),
            // pins.dotstar_ci.into_push_pull_output(&mut pins.port),
            // pins.d13.into_floating_input(&mut pins.port),
            // // Extrenal
            // pins.d0.into_push_pull_output(&mut pins.port),
            // pins.d1.into_push_pull_output(&mut pins.port),
            // pins.d2.into_floating_input(&mut pins.port),
            pins.d3.into_floating_input(&mut pins.port),
            pins.d4.into_floating_input(&mut pins.port),
        );

        let mut uart = hal::uart(
            &mut clocks,
            921_600.hz(),
            device.SERCOM0,
            &mut dp.PM,
            pins.d3,
            pins.d4,
            &mut pins.port,
        );

        // let mut rx_led = pins.rx_led.into_open_drain_output(&mut pins.port);
        // let mut tx_led = pins.tx_led.into_open_drain_output(&mut pins.port);
        //
        // tx_led.set_high().unwrap();
        // rx_led.set_high().unwrap();

        dbgprint!("done init");

        init::LateResources {
            // blue_led: led,
            // tx_led,
            // rx_led,
            uart,
        }
    }

    #[task(binds = SERCOM0, resources = [uart])]
    fn SERCOM0(c: SERCOM0::Context) {
        // c.resources.rx_led.set_low().unwrap();
        // let data = match block!(c.resources.uart.read()) {
        //     Ok(v) => {
        //         c.resources.rx_led.set_high().unwrap();
        //         v
        //     }
        //     Err(_) => 0 as u8,
        // };
        //
        // c.resources.tx_led.set_low().unwrap();
        // match block!(c.resources.uart.write(data)) {
        //     Ok(_) => {
        //         c.resources.tx_led.set_high().unwrap();
        //     }
        //     Err(_) => unimplemented!(),
        // }
    }
};
