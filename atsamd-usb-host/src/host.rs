use atsamd_hal::{
    calibration::{usb_transn_cal, usb_transp_cal, usb_trim_cal},
    clock::{ClockGenId, ClockSource, GenericClockController},
    gpio::{self, Floating, Input, OpenDrain, Output},
    target_device::{PM, USB},
};

use embedded_hal::digital::v2::OutputPin;

use sync_thumbv6m::alloc::Arc;
use sync_thumbv6m::spin::SpinMutex;
use crate::usb_host::address::AddressPool;

use crate::pipe::{PipeTable};

use crate::usb_host::device::{Device};
use crate::usb_host::{Driver, Endpoint, RequestCode, RequestType, TransferError, USBHost, WValue};
use crate::usb_host::parser::DescriptorParser;

#[derive(Clone, Copy, Debug, PartialEq, defmt::Format)]
pub enum HostEvent {
    NoEvent,
    Detached,
    Attached,
    RamAccess,
    UpstreamResume,
    DownResume,
    WakeUp,
    Reset,
    StartOfFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, defmt::Format)]
enum State {
    WaitForDevice,
    WaitResetComplete,
    // WaitSOF(u64),
    Running,
    Error,
}

pub struct SAMDHost {
    usb: USB,
    task_state: State,

    // Need chunk of RAM for USB pipes, which gets used with DESCADD register
    pipe_table: PipeTable,

    addr_pool: Arc<SpinMutex<AddressPool>>,

    _dm_pad: gpio::Pa24<gpio::PfG>,
    _dp_pad: gpio::Pa25<gpio::PfG>,
    _sof_pad: Option<gpio::Pa23<gpio::PfG>>,
    host_enable_pin: Option<gpio::Pa28<Output<OpenDrain>>>,
    millis: fn() -> u64,
}

// The maximum size configuration descriptor we can handle.
const CONFIG_BUFFER_LEN: usize = 256;

// FIXME why isn't atsamd21e::USB Sync ?
unsafe impl Sync for SAMDHost {}

pub struct Pins {
    dm_pin: gpio::Pa24<Input<Floating>>,
    dp_pin: gpio::Pa25<Input<Floating>>,
    sof_pin: Option<gpio::Pa23<Input<Floating>>>,
    host_enable_pin: Option<gpio::Pa28<Input<Floating>>>,
}

impl Pins {
    pub fn new(
        dm_pin: gpio::Pa24<Input<Floating>>,
        dp_pin: gpio::Pa25<Input<Floating>>,
        sof_pin: Option<gpio::Pa23<Input<Floating>>>,
        host_enable_pin: Option<gpio::Pa28<Input<Floating>>>,
    ) -> Self {
        Self {
            dm_pin,
            dp_pin,
            sof_pin,
            host_enable_pin,
        }
    }
}

impl SAMDHost {
    pub fn new(
        usb: USB,
        pins: Pins,
        port: &mut gpio::Port,
        clocks: &mut GenericClockController,
        power: &mut PM,
        millis: fn() -> u64,
    ) -> Self {
        power.apbbmask.modify(|_, w| w.usb_().set_bit());

        clocks.configure_gclk_divider_and_source(ClockGenId::GCLK6, 1, ClockSource::DFLL48M, false);
        let gclk6 = clocks.get_gclk(ClockGenId::GCLK6).expect("Could not get clock 6");
        clocks.usb(&gclk6);

        SAMDHost {
            usb,
            task_state: State::WaitForDevice,
            pipe_table: PipeTable::new(),
            addr_pool: Arc::new(SpinMutex::new(AddressPool::new())),

            _dm_pad: pins.dm_pin.into_function_g(port),
            _dp_pad: pins.dp_pin.into_function_g(port),
            _sof_pad: pins.sof_pin.map(|p| p.into_function_g(port)),
            host_enable_pin: pins.host_enable_pin.map(|p| p.into_open_drain_output(port)),
            millis,
        }
    }

    /// Low-Level USB Host Interrupt service method
    /// Any Event returned by should be sent to process_event()
    /// then fsm_tick() should be called for each event or once if no event at all
    pub fn get_event(&self) -> HostEvent {
        let flags = self.usb.host().intflag.read();

        if flags.ddisc().bit_is_set() {
            self.usb.host().intflag.write(|w| w.ddisc().set_bit());
            HostEvent::Detached
        } else if flags.dconn().bit_is_set() {
            self.usb.host().intflag.write(|w| w.dconn().set_bit());
            HostEvent::Attached
        } else if flags.ramacer().bit_is_set() {
            self.usb.host().intflag.write(|w| w.ramacer().set_bit());
            HostEvent::RamAccess
        } else if flags.uprsm().bit_is_set() {
            self.usb.host().intflag.write(|w| w.uprsm().set_bit());
            HostEvent::UpstreamResume
        } else if flags.dnrsm().bit_is_set() {
            self.usb.host().intflag.write(|w| w.dnrsm().set_bit());
            HostEvent::DownResume
        } else if flags.wakeup().bit_is_set() {
            self.usb.host().intflag.write(|w| w.wakeup().set_bit());
            HostEvent::WakeUp
        } else if flags.rst().bit_is_set() {
            self.usb.host().intflag.write(|w| w.rst().set_bit());
            HostEvent::Reset
        } else if flags.hsof().bit_is_set() {
            self.usb.host().intflag.write(|w| w.hsof().set_bit());
            HostEvent::StartOfFrame
        } else {
            HostEvent::NoEvent
        }
    }

    pub async fn update(&mut self, host_event: HostEvent, drivers: &mut [&'static mut (dyn Driver + Send + Sync)]) {
        let prev = self.task_state;

        self.task_state = match (host_event, self.task_state) {
            (HostEvent::Detached, _) => {
                debug!("USB Device disconnected, resetting host");
                self.reset();
                State::WaitForDevice
            }

            (HostEvent::Attached, _) => {
                debug!("USB Device connected");
                self.usb.host().ctrlb.modify(|_, w| w.busreset().set_bit());
                while self.usb.host().ctrlb.read().busreset().bit_is_set() {
                    runtime::delay_ms(11).await;
                }

                self.usb.host().ctrlb.modify(|_, w| w.sofe().set_bit());
                match self.configure_dev(drivers) {
                    Ok(_) => State::Running,

                    Err(e) => {
                        warn!("USB Device Configuration Error: {:?}", e);
                        State::WaitForDevice
                    }
                }
            }

            (HostEvent::StartOfFrame, State::Running) => {
                trace!("USB Tick");
                for d in drivers {
                    d.tick(self);
                }
                self.task_state
            }

            _ => self.task_state
        };

        if prev != self.task_state {
            debug!("USB Event [{:?}] Update [{:?}] -> [{:?}]", host_event, prev, self.task_state);
        }
    }

    /// reset host state registers only
    /// called on init or on error
    /// bus reset itself managed by hardware
    pub fn reset(&mut self) {
        self.usb.host().ctrla.write(|w| w.swrst().set_bit());
        while self.usb.host().syncbusy.read().swrst().bit_is_set() {}

        self.usb.host().ctrla.modify(|_, w| w.mode().host());

        unsafe {
            self.usb.host().padcal.write(|w| {
                w.transn().bits(usb_transn_cal());
                w.transp().bits(usb_transp_cal());
                w.trim().bits(usb_trim_cal())
            });
        }

        self.usb.host().ctrlb.modify(|_, w| w.spdconf().normal());
        self.usb.host().ctrla.modify(|_, w| w.runstdby().set_bit());

        unsafe { self.usb.host().descadd.write(|w| w.bits(&self.pipe_table as *const _ as u32)); }

        if let Some(host_enable_pin) = &mut self.host_enable_pin {
            host_enable_pin.set_high().expect("USB Reset [host enable pin]");
        }

        self.usb.host().intenset.write(|w| {
            w.dconn().set_bit();
            w.ddisc().set_bit();
            w.wakeup().set_bit()
            // w.uprsm().set_bit();
            // w.dnrsm().set_bit();
            // w.rst().set_bit();
            // w.hsof().set_bit()
        });

        self.usb.host().ctrla.modify(|_, w| w.enable().set_bit());
        while self.usb.host().syncbusy.read().enable().bit_is_set() {}
        self.usb.host().ctrlb.modify(|_, w| w.vbusok().set_bit());
        debug!("USB Host Reset");
    }

    fn configure_dev(&mut self, drivers: &mut [&'static mut (dyn Driver + Send + Sync)]) -> Result<(), TransferError> {
        debug!("USB Configuring Device");
        let max_bus_packet_size: u16 = match self.usb.host().status.read().speed().bits() {
            0x0 => 64,
            _ => 8,
        };
        let mut device = Device::new(max_bus_packet_size);
        let dev_desc = device.get_device_descriptor(self)?;

        let dev_addr = self.addr_pool.lock().take_next().ok_or(TransferError::Permanent("Out of USB addr"))?;
        device.set_device_address(self, dev_addr)?;

        let mut cfg_buf = [0; CONFIG_BUFFER_LEN];
        let _len = device.get_configuration_descriptors(self, 0, &mut cfg_buf)?;
        let mut parser = DescriptorParser::new(&cfg_buf);

        // TODO store device state?
        for d in drivers.iter_mut() {
            match d.connected(self, &mut device, &dev_desc, &mut parser) {
                Ok(true) => break,
                Err(e) => error!("Driver failed on connect {:?}", e),
                _ => {}
            };
            parser.rewind()
        }
        Ok(())
    }
}

impl USBHost for SAMDHost {
    fn get_host_id(&self) -> u8 {
        // TODO incremental host ids
        0
    }

    fn control_transfer(&mut self, ep: &mut Endpoint, req_type: RequestType, req_code: RequestCode,
                        w_value: WValue, w_index: u16, buf: Option<&mut [u8]>) -> Result<usize, TransferError> {
        let mut pipe = self.pipe_table.pipe_for(self.usb.host_mut(), ep, self.millis);
        Ok(pipe.control_transfer(ep, req_type, req_code, w_value, w_index, buf)?)
    }

    fn in_transfer(&mut self, ep: &mut Endpoint, buf: &mut [u8]) -> Result<usize, TransferError> {
        let mut pipe = self.pipe_table.pipe_for(self.usb.host_mut(), ep, self.millis);
        Ok(pipe.in_transfer(ep, buf)?)
    }

    fn out_transfer(&mut self, ep: &mut Endpoint, buf: &[u8]) -> Result<usize, TransferError> {
        let mut pipe = self.pipe_table.pipe_for(self.usb.host_mut(), ep, self.millis);
        Ok(pipe.out_transfer(ep, buf)?)
    }
}

