use std::sync::Arc;
use crate::config::Config;
use crate::logger::{log, ConsoleAndFileLogger, LogLevel};
use crate::mail_dispatcher::{Mailer, SocketMailer};
use crate::smtp::TutaMailTransferAgent;

mod logger;
mod config;
mod mail_dispatcher;
mod error;
mod smtp;
mod mime;

pub const DEFAULT_SOCKET_PATH: &str = "/tmp/stone-bridge.sock";


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

    let mailer = SocketMailer::new(DEFAULT_SOCKET_PATH).expect("failed to build sender client");
    let mta = TutaMailTransferAgent::new(Arc::new(mailer), config.bind_address, config.hostname);
    mta.start().await;
}
