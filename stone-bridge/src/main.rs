use std::sync::Arc;
use crate::config::Config;
use crate::logger::{log, ConsoleAndFileLogger, LogLevel};

mod logger;
mod config;

#[tokio::main]
async fn main() {
    let config = Config::load();

    let logger = ConsoleAndFileLogger::new(
        LogLevel::from(config.log_level.as_str()),
        config.log_path.clone(),
    );

    if logger::install(Arc::new(logger)).is_err() {
        unreachable!("logger is installed only once, in main");
    }

    log::info!("Loaded config and initialized logger");

}
