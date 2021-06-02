/// using entire gpio port, faster?

use display_interface::v2::*;
use display_interface::DisplayError;

use embedded_hal::digital::v2::{InputPin, OutputPin};
use ili9486::io::IoPin;
use display_interface::v2::{WriteMode, WriteInterface, ReadInterface};
use stm32f4::stm32f411::gpioa::ODR;

pub struct NoGPIO {}

impl ReadInterface<u8> for NoGPIO {
    fn read_stream(&mut self, f: &mut dyn FnMut(u8) -> bool) -> Result<(), DisplayError> {
        Ok(())
    }
}

impl WriteInterface<u8> for NoGPIO {
    #[inline(always)]
    fn write_stream<'a>(&mut self, mode: WriteMode, func: &mut dyn FnMut() -> Option<&'a u8>) -> Result<(), DisplayError> {
        Ok(())
    }
}
