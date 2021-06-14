
use lvgl::LvError;
use display_interface::DisplayError;

pub mod gui;


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
