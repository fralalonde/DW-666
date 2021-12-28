use alloc::boxed::Box;
use core::fmt::Write;

use log::{LevelFilter, Metadata, Record};
use sync_thumbv6m::spin::SpinMutex;

/// An RTT-based logger implementation.
pub struct SerialLogging {
    port: SpinMutex<Box<dyn Write + Sync + Send>>,
}

impl log::Log for SerialLogging {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            core::writeln!(self.port.lock(), "{} - {}", record.level(), record.args()).unwrap();
        }
    }

    fn flush(&self) {}
}

static mut LOGGER: Option<SerialLogging> = None;

pub fn init(port: Box<dyn Write + Sync + Send>) {
    unsafe { LOGGER.replace(SerialLogging { port: SpinMutex::new(port) }) };
    log::set_max_level(LevelFilter::Trace);
    unsafe { log::set_logger_racy(LOGGER.as_ref().unwrap_unchecked()).unwrap(); }
}