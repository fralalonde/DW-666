use usb_device::bus::UsbBus;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::UsbDevice;
use usb_device::device::UsbVidPid;
use usb_device::prelude::UsbDeviceBuilder;
use usbd_midi::data::usb::constants::USB_CLASS_NONE;
use usbd_midi::midi_device::MidiClass;
use stm32f1xx_hal::gpio::{Input, Floating};

use cortex_m::asm::delay;
use embedded_hal::digital::v2::OutputPin;
use stm32f1xx_hal::gpio::gpiob::{PB11, PB12, PB13, PB14, PB15};
use stm32f1xx_hal::gpio::PullUp;
use stm32f1xx_hal::usb::Peripheral;

/// Configures the usb device as seen by the operating system.
pub fn configure_usb<'a, B: UsbBus>(
    usb_bus: &'a UsbBusAllocator<B>,
) -> UsbDevice<'a, B> {
    let usb_vid_pid = UsbVidPid(0x16c0, 0x27dd);
    let usb_dev = UsbDeviceBuilder::new(usb_bus, usb_vid_pid)
        .manufacturer("Roto")
        .product("USB MIDI Router")
        .serial_number("123")
        .device_class(USB_CLASS_NONE)
        .build();
    usb_dev
}

