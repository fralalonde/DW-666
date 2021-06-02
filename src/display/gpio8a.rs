/// using entire gpio port, faster?

use display_interface::v2::*;
use display_interface::DisplayError;

use embedded_hal::digital::v2::{InputPin, OutputPin};
use ili9486::io::IoPin;
use display_interface::v2::{WriteMode, WriteInterface, ReadInterface};
use stm32f4::stm32f411::gpioa::ODR;

macro_rules! wrap_input_err {
    ($expr:expr) => {
        $expr.map_err(|_e| DisplayError::BusReadError);
    };
}

macro_rules! wrap_output_err {
    ($expr:expr) => {
        $expr.map_err(|_e| DisplayError::BusWriteError);
    };
}

pub trait RawGPIO {
    fn init(&mut self);
    fn write_mode(&mut self, enable: bool);
    fn read_byte(&mut self) -> u8;
    fn write_port(&mut self, byte: u16);
}

const MODE_INPUT: u32 = 0x00000000;
const MODE_OUTPUT: u32 = 0b_0101_0101_0101_0101_0101_0101_0101_0101;
const TYPE_OUT: u32 = 0x0000FFFF;
const PULL_DOWN_INPUT: u32 = 0b_1010_1010_1010_1010_1010_1010_1010_1010;
const NO_PULL: u32 = 0b_0;
const OUTPUT_SPEED: u32 = 0x0000FFFF;

impl RawGPIO for stm32f4::stm32f411::GPIOA {
    fn init(&mut self) {
        self.otyper.modify(|r, w| unsafe {
            w.bits(r.bits() | TYPE_OUT)
        });
    }

    fn write_mode(&mut self, enable: bool) {
        if enable {
            self.pupdr.modify(|r, w| unsafe {
                w.bits(r.bits() | NO_PULL)
            });
            self.moder.modify(|r, w| unsafe {
                w.bits(r.bits() | MODE_OUTPUT)
            });
        } else {
            self.pupdr.modify(|r, w| unsafe {
                w.bits(r.bits() | PULL_DOWN_INPUT)
            });
            self.moder.modify(|r, w| unsafe {
                w.bits(r.bits() | MODE_INPUT)
            });
        }
    }

    fn read_byte(&mut self) -> u8 {
        self.idr.read().bits() as u8
    }

    fn write_port(&mut self, byte: u16) {
        self.odr.write(|w| unsafe { w.bits(byte as u32) })
    }
}

impl RawGPIO for stm32f4::stm32f411::GPIOB {
    fn init(&mut self) {
        self.otyper.modify(|r, w| unsafe {
            w.bits(r.bits() | TYPE_OUT)
        });
    }

    fn write_mode(&mut self, enable: bool) {
        if enable {
            // &(*$GPIOX::ptr()).pupdr.modify(|r, w| {
            //     w.bits((r.bits() & !(0b11 << offset)) | (0b00 << offset))
            // });
            // &(*$GPIOX::ptr()).otyper.modify(|r, w| {
            //     w.bits(r.bits() | (0b1 << $i))
            // });
            // &(*$GPIOX::ptr()).moder.modify(|r, w| {
            //     w.bits((r.bits() & !(0b11 << offset)) | (0b01 << offset))
            // })
            self.pupdr.modify(|r, w| unsafe {
                w.bits(r.bits() | NO_PULL)
            });

            self.moder.modify(|r, w| unsafe {
                w.bits(r.bits() | MODE_OUTPUT)
            });
        } else {
            //     &(*$GPIOX::ptr()).pupdr.modify(|r, w| {
            //         w.bits((r.bits() & !(0b11 << offset)) | (0b10 << offset))
            //     });
            //     &(*$GPIOX::ptr()).moder.modify(|r, w| {
            //         w.bits((r.bits() & !(0b11 << offset)) | (0b00 << offset))
            //     })
            self.pupdr.modify(|r, w| unsafe {
                w.bits(r.bits() | PULL_DOWN_INPUT)
            });
            self.moder.modify(|r, w| unsafe {
                w.bits(r.bits() | MODE_INPUT)
            });
        }
    }

    fn read_byte(&mut self) -> u8 {
        self.idr.read().bits() as u8
    }
    fn write_port(&mut self, value: u16) {
        self.odr.write(|w| unsafe { w/*.odr3().set_bit()*/.bits(value as u32 & 0xFFFF) })
    }
}

pub struct GPIO8aParallelInterface<PORT, CS, DCX, RDX, WRX>
    where PORT: RawGPIO, CS: IoPin, DCX: IoPin, RDX: IoPin, WRX: IoPin,
{
    port: PORT,
    cs: CS,
    dcx: DCX,
    rdx: RDX,
    wrx: WRX,
}

impl<PORT, CS, DCX, RDX, WRX> GPIO8aParallelInterface<PORT, CS, DCX, RDX, WRX>
    where PORT: RawGPIO, CS: IoPin, DCX: IoPin, RDX: IoPin, WRX: IoPin,
{
    pub fn new(mut port: PORT, mut cs: CS, mut dcx: DCX, mut rdx: RDX, mut wrx: WRX) -> Result<GPIO8aParallelInterface<PORT, CS, DCX, RDX, WRX>, DisplayError, > {
        wrap_output_err!(dcx.into_output().set_high())?;
        wrap_output_err!(cs.into_output().set_high())?;
        wrap_output_err!(rdx.into_output().set_high())?;
        wrap_output_err!(wrx.into_output().set_high())?;

        // port.init();

        Ok(GPIO8aParallelInterface {
            port,
            cs,
            dcx,
            rdx,
            wrx,
        })
    }
}

impl<PORT, CS, DCX, RDX, WRX> ReadInterface<u8> for GPIO8aParallelInterface<PORT, CS, DCX, RDX, WRX>
    where PORT: RawGPIO, CS: IoPin, DCX: IoPin, RDX: IoPin, WRX: IoPin,
{
    fn read_stream(&mut self, f: &mut dyn FnMut(u8) -> bool) -> Result<(), DisplayError> {
        let cs = self.cs.into_output();
        let rdx = self.rdx.into_output();
        let dcx = self.dcx.into_output();
        let wrx = self.wrx.into_output();

        wrap_output_err!(rdx.set_high())?;
        wrap_output_err!(wrx.set_high())?;
        wrap_output_err!(cs.set_low())?;
        wrap_output_err!(dcx.set_high())?;

        self.port.write_mode(false);
        loop {
            wrap_output_err!(rdx.set_low())?;
            let mut byte: u8 = self.port.read_byte();
            wrap_output_err!(rdx.set_high())?;
            let read_more = f(byte);
            if !read_more {
                break;
            }
        }
        wrap_output_err!(dcx.set_low())?;
        wrap_output_err!(cs.set_high())?;

        Ok(())
    }
}

impl<PORT, CS, DCX, RDX, WRX> WriteInterface<u8>
for GPIO8aParallelInterface<PORT, CS, DCX, RDX, WRX>
    where PORT: RawGPIO, CS: IoPin, DCX: IoPin, RDX: IoPin, WRX: IoPin,
{
    #[inline(always)]
    fn write_stream<'a>(&mut self, mode: WriteMode, func: &mut dyn FnMut() -> Option<&'a u8>) -> Result<(), DisplayError> {
        let cs = self.cs.into_output();
        let rdx = self.rdx.into_output();
        let dcx = self.dcx.into_output();
        let wrx = self.wrx.into_output();

        wrap_output_err!(rdx.set_high())?;
        wrap_output_err!(wrx.set_high())?;
        wrap_output_err!(cs.set_low())?;

        match mode {
            WriteMode::Command => {
                wrap_output_err!(dcx.set_low())?;
            }
            _ => {
                wrap_output_err!(dcx.set_high())?;
            }
        }

        self.port.write_mode(true);
        loop {
            match func() {
                Some(byte) => {
                    wrap_output_err!(wrx.set_low())?;
                    self.port.write_port(*byte as u16);
                    wrap_output_err!(wrx.set_high())?;
                }
                None => {
                    break;
                }
            }
        }

        match mode {
            WriteMode::Command => {
                wrap_output_err!(dcx.set_high())?;
            }
            _ => {
                wrap_output_err!(dcx.set_low())?;
            }
        }

        wrap_output_err!(wrx.set_high())?;
        wrap_output_err!(cs.set_high())?;

        Ok(())
    }
}
