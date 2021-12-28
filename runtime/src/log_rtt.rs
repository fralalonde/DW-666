use panic_rtt_target as _;

use log::{LevelFilter, Metadata, Record};

/// An RTT-based logger implementation.
pub struct RTTLogger {}

impl log::Log for RTTLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            rtt_target::rprintln!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: RTTLogger = RTTLogger {};

pub fn init() {
    rtt_target::rtt_init_print!(NoBlockSkip, 1024);
    log::set_max_level(LevelFilter::Trace);
    unsafe { log::set_logger_racy(&LOGGER).unwrap(); }
}