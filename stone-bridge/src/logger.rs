use std::{
    fmt::Display,
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

static LOGGER: OnceLock<Arc<dyn Log>> = OnceLock::new();

/// Installs the global logger. Returns the rejected logger if one was already
/// installed.
pub fn install(logger: Arc<dyn Log>) -> Result<(), Arc<dyn Log>> {
    LOGGER.set(logger)
}

/// Used by the log macros; not meant to be called directly.
pub fn dispatch(level: LogLevel, msg: &str) {
    match LOGGER.get() {
        Some(logger) => logger.log(level, msg),
        None => eprintln!("[{}] {}", level, msg),
    }
}

pub trait Log: Send + Sync {
    fn log(&self, level: LogLevel, msg: &str);
}

pub struct ConsoleAndFileLogger {
    level: LogLevel,
    log_file_path: PathBuf,
}

impl Log for ConsoleAndFileLogger {
    fn log(&self, level: LogLevel, msg: &str) {
        if level > self.level {
            return;
        }

        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.log_file_path)
            .expect("Could not open log file");

        let date = chrono::Local::now();

        let log_line = format!(
            "[{}] [{}] {}",
            date.format("%Y-%m-%d %H:%M:%S%.3f"),
            level,
            msg
        );
        println!("{}", &log_line);
        file.write_all(log_line.as_bytes()).unwrap();
    }
}

impl ConsoleAndFileLogger {
    pub fn new(level: LogLevel, log_file_path: PathBuf) -> Self {
        if !log_file_path.exists() {
            File::create(&log_file_path).unwrap_or_else(|e| {
                panic!(
                    "Error while creating log file at {}: {}",
                    log_file_path.display(),
                    e
                )
            });
        }
        Self {
            level,
            log_file_path,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl From<&str> for LogLevel {
    fn from(value: &str) -> Self {
        match value {
            v if v.to_lowercase() == "error" => LogLevel::Error,
            v if v.to_lowercase() == "warn" => LogLevel::Warn,
            v if v.to_lowercase() == "info" => LogLevel::Info,
            v if v.to_lowercase() == "debug" => LogLevel::Debug,
            _ => LogLevel::Debug,
        }
    }
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        {
            match self {
                LogLevel::Error => write!(f, "ERR"),
                LogLevel::Warn => write!(f, "WARN"),
                LogLevel::Info => write!(f, "INFO"),
                LogLevel::Debug => write!(f, "DEBUG"),
            }
        }
    }
}

macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::logger::dispatch($crate::logger::LogLevel::Debug, &format!($($arg)*))
    };
}

macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logger::dispatch($crate::logger::LogLevel::Info, &format!($($arg)*))
    };
}

macro_rules! warning {
    ($($arg:tt)*) => {
        $crate::logger::dispatch($crate::logger::LogLevel::Warn, &format!($($arg)*))
    };
}

macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logger::dispatch($crate::logger::LogLevel::Error, &format!($($arg)*))
    };
}

pub mod log {
    pub(crate) use {debug, error, info, warning};
}

#[cfg(test)]
mod tests {
    use super::{log, ConsoleAndFileLogger, Log, LogLevel};

    #[test]
    fn console_and_file_logger_filters_by_level() {
        let path = std::env::temp_dir().join(format!(
            "stonebridge-logger-test-{}.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let logger = ConsoleAndFileLogger::new(LogLevel::Info, path.clone());

        logger.log(LogLevel::Debug, "dropped");
        logger.log(LogLevel::Info, "kept-info");
        logger.log(LogLevel::Error, "kept-error");

        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(!contents.contains("dropped"));
        assert!(contents.contains("[INFO] kept-info"));
        assert!(contents.contains("[ERR] kept-error"));
    }

    #[test]
    fn macros_do_not_panic_without_installed_logger() {
        log::debug!("debug {}", 1);
        log::info!("info");
        log::warning!("warning");
        log::error!("error");
    }
}
