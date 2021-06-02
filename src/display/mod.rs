
use lvgl::LvError;
use core::convert::Infallible;
use display_interface::DisplayError;

pub mod gpio8a;
pub mod gpio8b;
pub mod gui;
pub mod rotate;
pub mod nogpio;

#[derive(Debug)]
pub enum GuiError {
    LvError,
    DisplayError,
}

impl From<LvError> for GuiError {
    fn from(_: LvError) -> Self {
        GuiError::LvError
    }
}

impl From<DisplayError> for GuiError {
    fn from(_: DisplayError) -> Self {
        GuiError::DisplayError
    }
}
