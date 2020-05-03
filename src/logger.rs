use log::{set_logger, set_max_level, LevelFilter, Log, Metadata, Record, SetLoggerError};

pub struct Logger;

impl Logger {
    pub fn init(filter: LevelFilter) -> Result<(), SetLoggerError> {
        set_logger(&LOGGER).map(|()| set_max_level(filter))
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!(
                "|{}| {} -- {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger;
